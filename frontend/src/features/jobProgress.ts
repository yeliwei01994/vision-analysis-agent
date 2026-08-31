import type { JobProgressEvent, JobStage } from '../types/events';

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

function progressTimestamp(updated_at: string | number): number {
  return typeof updated_at === 'number' ? updated_at : Date.parse(updated_at);
}

function normalizedProgress(progress: number): number {
  return progress > 1 ? progress / 100 : progress;
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

export function mergeJobProgress(previous: JobProgressEvent, next: JobProgressEvent): JobProgressEvent {
  if (previous.job_id !== next.job_id) {
    return previous;
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
