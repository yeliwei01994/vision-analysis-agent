import { JobProgressBar } from './JobProgressBar';
import { jobStageLabel, jobStatusLabel } from './jobProgress';
import type { JobProgressEvent, VideoJob } from '../types/events';

type JobTaskCardProps = {
  job: VideoJob;
  progressEvent?: JobProgressEvent | null;
  onOpen: (jobId: string) => void;
  onRetry: (jobId: string) => void;
  onCancel?: ((jobId: string) => void) | null;
  onEdit: (job: VideoJob) => void;
  onDelete: (job: VideoJob) => void;
};

const trackableStatuses = new Set<JobProgressEvent['status']>(['pending', 'processing', 'completed', 'failed', 'cancelled']);
const retryableStatuses = new Set<JobProgressEvent['status']>(['failed', 'cancelled']);
const activeStatuses = new Set<JobProgressEvent['status']>(['pending', 'processing']);

function isTrackableStatus(status: string): status is JobProgressEvent['status'] {
  return trackableStatuses.has(status as JobProgressEvent['status']);
}

function effectiveStatus(job: VideoJob, progressEvent?: JobProgressEvent | null): JobProgressEvent['status'] {
  if (progressEvent) {
    return progressEvent.status;
  }

  return isTrackableStatus(job.status) ? job.status : 'pending';
}

function formatDuration(ms: number) {
  if (!Number.isFinite(ms) || ms < 0) {
    return null;
  }

  const totalSeconds = Math.round(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return [hours, minutes, seconds].map((value) => value.toString().padStart(2, '0')).join(':');
  }

  return [minutes, seconds].map((value) => value.toString().padStart(2, '0')).join(':');
}

function formatRecentUpdate(updatedAt?: string | number) {
  if (updatedAt === undefined) {
    return null;
  }

  const date = new Date(updatedAt);
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  const parts = [
    date.getFullYear(),
    (date.getMonth() + 1).toString().padStart(2, '0'),
    date.getDate().toString().padStart(2, '0'),
  ];
  const time = [
    date.getHours().toString().padStart(2, '0'),
    date.getMinutes().toString().padStart(2, '0'),
    date.getSeconds().toString().padStart(2, '0'),
  ];

  return `${parts.join('-')} ${time.join(':')}`;
}

function progressValue(job: VideoJob, progressEvent?: JobProgressEvent | null) {
  if (progressEvent?.progress === null) {
    return job.progress;
  }

  if (typeof progressEvent?.progress === 'number') {
    return progressEvent.progress > 1 ? progressEvent.progress : progressEvent.progress * 100;
  }

  return job.progress;
}

export function JobTaskCard({
  job,
  progressEvent,
  onOpen,
  onRetry,
  onCancel,
  onEdit,
  onDelete,
}: JobTaskCardProps) {
  const status = effectiveStatus(job, progressEvent);
  const stage = progressEvent?.stage ? jobStageLabel(progressEvent.stage) : null;
  const statusText = jobStatusLabel(status);
  const recentUpdate = formatRecentUpdate(progressEvent?.updated_at);
  const eta = formatDuration(progressEvent?.estimated_remaining_ms ?? NaN);
  const indeterminate = progressEvent?.progress === null;
  const canRetry = retryableStatuses.has(status);
  const canCancel = Boolean(onCancel) && activeStatuses.has(status);
  const compact = !activeStatuses.has(status);

  return (
    <article className={`job-task-card${compact ? ' compact' : ''}`} aria-label={job.filename}>
      <div className="job-task-card-top">
        <div>
          <p className="eyebrow">VIDEO JOB</p>
          <h3>{job.filename}</h3>
        </div>
        <span className={`job-status-badge ${status}`}>{statusText}</span>
      </div>

      <div className="job-task-meta">
        <div>
          <span>当前阶段</span>
          <strong>{stage ?? statusText}</strong>
        </div>
        <div>
          <span>视频时长</span>
          <strong>{formatDuration(job.duration_ms) ?? '--'}</strong>
        </div>
        {recentUpdate && (
          <div>
            <span>最近更新</span>
            <strong>{recentUpdate}</strong>
          </div>
        )}
        {eta && (
          <div>
            <span>预计剩余</span>
            <strong>{eta}</strong>
          </div>
        )}
      </div>

      <div className="job-progress-copy">
        <div className="job-progress-heading">
          <span>处理进度</span>
          <strong>{indeterminate ? '处理中…' : `${Math.max(0, Math.min(100, Math.round(progressValue(job, progressEvent)))).toString()}%`}</strong>
        </div>
        <JobProgressBar progress={progressValue(job, progressEvent)} indeterminate={indeterminate} label={`${job.filename} 进度`} />
        {progressEvent?.message && <p>{progressEvent.message}</p>}
      </div>

      <div className="job-task-actions">
        <button className="job-action-button" type="button" onClick={() => onOpen(job.id)} aria-label={`打开 ${job.filename}`}>
          打开
        </button>
        {canRetry && (
          <button className="job-action-button" type="button" onClick={() => onRetry(job.id)} aria-label={`重试 ${job.filename}`}>
            重试
          </button>
        )}
        {canCancel && (
          <button className="job-action-button" type="button" onClick={() => onCancel?.(job.id)} aria-label={`取消 ${job.filename}`}>
            取消
          </button>
        )}
        <button className="job-action-button" type="button" onClick={() => onEdit(job)} aria-label={`编辑 ${job.filename}`}>
          编辑
        </button>
        <button
          className="job-action-button"
          type="button"
          onClick={() => onDelete(job)}
          aria-label={`删除 ${job.filename}`}
          disabled={status === 'processing'}
        >
          删除
        </button>
      </div>
    </article>
  );
}
