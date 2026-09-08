import type { JobProgressEvent, JobProgressSnapshot, JobStage, VideoJob } from '../types/events';

type TrackableJobStatus = JobProgressEvent['status'];

type MergedJobsSnapshot = {
  jobsById: Record<string, VideoJob>;
  progressById: Record<string, JobProgressEvent>;
};

type TimestampedProgress = JobProgressEvent & {
  progress: number;
  updated_at: string | number;
  timestamp: number;
};

const stageLabels: Record<JobStage, string> = {
  preparing: '正在准备视频',
  reading: '正在读取视频',
  extracting_frames: '正在抽取关键帧',
  detecting: '正在进行目标检测',
  analyzing_events: '正在分析事件',
  generating_playback: '正在生成检测回放',
  finalizing: '正在整理分析结果',
};

const statusLabels: Record<JobProgressEvent['status'], string> = {
  pending: '等待处理',
  processing: '正在处理',
  completed: '处理完成',
  failed: '处理失败',
  cancelled: '已取消',
};

const terminalStatuses = new Set<JobProgressEvent['status']>(['completed', 'failed', 'cancelled']);
const trackableStatuses = new Set<TrackableJobStatus>(['pending', 'processing', 'completed', 'failed', 'cancelled']);

function progressTimestamp(updated_at: string | number): number {
  return typeof updated_at === 'number' ? updated_at : Date.parse(updated_at);
}

function normalizedProgress(progress: number): number {
  return progress / 100;
}

export function progressPercent(progress: number): number {
  return Math.max(0, Math.min(100, Math.round(normalizedProgress(progress) * 100)));
}

function jobPercent(progress: number): number {
  return Math.max(0, Math.min(100, Math.round(progress)));
}

function isTrackableJobStatus(status: string): status is TrackableJobStatus {
  return trackableStatuses.has(status as TrackableJobStatus);
}

function isTimestampedProgress(event: JobProgressEvent): event is JobProgressEvent & { progress: number; updated_at: string | number } {
  if (event.progress === null || event.updated_at === undefined) {
    return false;
  }

  const timestamp = progressTimestamp(event.updated_at);
  return Number.isFinite(timestamp);
}

function toTimestampedProgress(event: JobProgressEvent & { progress: number; updated_at: string | number }): TimestampedProgress {
  return {
    ...event,
    progress: normalizedProgress(event.progress),
    timestamp: progressTimestamp(event.updated_at),
  };
}

function isTerminal(status: JobProgressEvent['status']) {
  return terminalStatuses.has(status);
}

export function isTerminalJobStatus(status: string): status is JobProgressEvent['status'] {
  return terminalStatuses.has(status as JobProgressEvent['status']);
}

export function mergeJobProgress(previous: JobProgressEvent, next: JobProgressEvent): JobProgressEvent {
  if (previous.job_id !== next.job_id) {
    return previous;
  }

  if ((next.attempt ?? 0) < (previous.attempt ?? 0)) {
    return previous;
  }

  if ((next.attempt ?? 0) > (previous.attempt ?? 0)) {
    return next;
  }

  if (next.sequence < previous.sequence) {
    return previous;
  }

  if (isTerminal(previous.status) && !isTerminal(next.status)) {
    return previous;
  }

  return next;
}

export function jobStageLabel(stage: JobStage): string {
  return stageLabels[stage];
}

export function jobStatusLabel(status: JobProgressEvent['status']): string {
  return statusLabels[status];
}

export function jobActivityLabel(progress?: JobProgressEvent | null, fallbackStatus?: string): string {
  if (progress?.stage) {
    return jobStageLabel(progress.stage);
  }

  if (progress) {
    return jobStatusLabel(progress.status);
  }

  if (fallbackStatus && (isTerminalJobStatus(fallbackStatus) || fallbackStatus === 'pending' || fallbackStatus === 'processing')) {
    return jobStatusLabel(fallbackStatus as JobProgressEvent['status']);
  }

  return '等待导入';
}

export function applyJobProgressToJob(job: VideoJob, progress: JobProgressEvent): VideoJob {
  return {
    ...job,
    status: progress.status,
    progress: progress.progress === null ? job.progress : progressPercent(progress.progress),
  };
}

function nextSyntheticSequence(previous?: JobProgressEvent): number {
  return (previous?.sequence ?? 0) + 1;
}

export function buildRetryProgress(jobId: string, previous?: JobProgressEvent): JobProgressEvent {
  return {
    job_id: jobId,
    status: 'pending',
    progress: 0,
    sequence: nextSyntheticSequence(previous),
    estimated_remaining_ms: null,
    attempt: (previous?.attempt ?? 0) + 1,
  };
}

export function buildJobProgressFromJob(job: VideoJob, previous?: JobProgressEvent): JobProgressEvent | undefined {
  if (!isTrackableJobStatus(job.status)) {
    return undefined;
  }

  return {
    job_id: job.id,
    status: job.status,
    progress: jobPercent(job.progress),
    sequence: nextSyntheticSequence(previous),
    stage: job.status === 'processing' ? previous?.stage : undefined,
    message: job.status === 'processing' ? previous?.message : undefined,
    estimated_remaining_ms: null,
    attempt: job.attempt ?? 0,
  };
}

export function mergeJobProgressSnapshot(
  previousJobsById: Record<string, VideoJob>,
  previousProgressById: Record<string, JobProgressEvent>,
  snapshot: JobProgressSnapshot,
): MergedJobsSnapshot {
  const jobsById = { ...previousJobsById };
  const progressById = { ...previousProgressById };

  for (const incoming of snapshot.jobs) {
    const current = progressById[incoming.job_id];
    const merged = current ? mergeJobProgress(current, incoming) : incoming;
    progressById[incoming.job_id] = merged;
    const job = jobsById[incoming.job_id];
    if (job) {
      jobsById[incoming.job_id] = applyJobProgressToJob(job, merged);
    }
  }

  return { jobsById, progressById };
}

function reconcileSnapshotProgress(previous: JobProgressEvent | undefined, snapshotJob: VideoJob): JobProgressEvent | undefined {
  if (!previous) {
    return undefined;
  }

  if (!isTrackableJobStatus(snapshotJob.status)) {
    return previous;
  }

  const snapshotProgress = jobPercent(snapshotJob.progress);
  const previousProgress = previous.progress === null ? null : progressPercent(previous.progress);
  const statusChanged = previous.status !== snapshotJob.status;
  const progressed = previousProgress === null ? snapshotProgress > 0 : snapshotProgress > previousProgress;
  const terminalAdvanced = isTerminalJobStatus(snapshotJob.status) && (previous.status !== snapshotJob.status || previousProgress !== snapshotProgress);

  if (!statusChanged && !progressed && !terminalAdvanced) {
    return previous;
  }

  return buildJobProgressFromJob(snapshotJob, previous);
}

export function mergeJobsSnapshot(
  previousJobsById: Record<string, VideoJob>,
  snapshot: VideoJob[],
  progressById: Record<string, JobProgressEvent>,
  activeJobId: string | null,
): MergedJobsSnapshot {
  const nextJobsById: Record<string, VideoJob> = {};
  const nextProgressById: Record<string, JobProgressEvent> = {};

  for (const job of snapshot) {
    const progress = reconcileSnapshotProgress(progressById[job.id], job) ?? progressById[job.id];
    if (progress) {
      nextProgressById[job.id] = progress;
    }
    nextJobsById[job.id] = progress ? applyJobProgressToJob(job, progress) : job;
  }

  if (!activeJobId || nextJobsById[activeJobId]) {
    return { jobsById: nextJobsById, progressById: nextProgressById };
  }

  const activeJob = previousJobsById[activeJobId];
  if (!activeJob || isTerminalJobStatus(activeJob.status)) {
    return { jobsById: nextJobsById, progressById: nextProgressById };
  }

  const activeProgress = progressById[activeJobId];
  if (activeProgress) {
    nextProgressById[activeJobId] = activeProgress;
  }

  return {
    jobsById: {
      ...nextJobsById,
      [activeJobId]: activeProgress ? applyJobProgressToJob(activeJob, activeProgress) : activeJob,
    },
    progressById: nextProgressById,
  };
}

export function orderJobsByPriority(jobsById: Record<string, VideoJob>, activeJobId: string | null): VideoJob[] {
  return Object.values(jobsById).sort((left, right) => {
    if (left.id === activeJobId) {
      return -1;
    }

    if (right.id === activeJobId) {
      return 1;
    }

    const leftActive = !isTerminalJobStatus(left.status);
    const rightActive = !isTerminalJobStatus(right.status);
    if (leftActive !== rightActive) {
      return leftActive ? -1 : 1;
    }

    return 0;
  });
}

export function estimateRemainingMs(history: JobProgressEvent[], current: JobProgressEvent): number | null {
  if (!isTimestampedProgress(current)) {
    return null;
  }

  const latest = toTimestampedProgress(current);
  const samples = history
    .filter(isTimestampedProgress)
    .map(toTimestampedProgress)
    .filter(sample => sample.timestamp < latest.timestamp)
    .sort((left, right) => left.timestamp - right.timestamp);

  if (!samples.length || latest.progress <= 0) {
    return null;
  }

  for (let index = samples.length - 1; index >= 0; index -= 1) {
    const previous = samples[index];
    const progressDelta = latest.progress - previous.progress;
    const timeDelta = latest.timestamp - previous.timestamp;

    if (progressDelta > 0 && timeDelta > 0) {
      return Math.max(0, Math.round(((1 - latest.progress) * timeDelta) / progressDelta));
    }
  }

  return null;
}
