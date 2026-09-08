import { useEffect, useId, useRef } from 'react';
import { JobProgressBar } from './JobProgressBar';
import { jobActivityLabel, jobStageLabel, jobStatusLabel, progressPercent } from './jobProgress';
import type { JobProgressEvent, JobStage, VideoJob } from '../types/events';

type JobTaskDrawerProps = {
  job: VideoJob;
  progressEvent?: JobProgressEvent | null;
  eventCount?: number | null;
  onClose: () => void;
  onRetry: (jobId: string) => void;
  onCancel?: ((jobId: string) => void) | null;
  onViewEvents?: ((jobId: string) => void) | null;
  onViewPlayback?: ((jobId: string) => void) | null;
};

const stages: JobStage[] = [
  'preparing',
  'reading',
  'extracting_frames',
  'detecting',
  'analyzing_events',
  'generating_playback',
  'finalizing',
];

const trackableStatuses = new Set<JobProgressEvent['status']>(['pending', 'processing', 'completed', 'failed', 'cancelled']);
const retryableStatuses = new Set<JobProgressEvent['status']>(['failed', 'cancelled']);
const activeStatuses = new Set<JobProgressEvent['status']>(['pending', 'processing']);

function effectiveStatus(job: VideoJob, progressEvent?: JobProgressEvent | null): JobProgressEvent['status'] {
  if (progressEvent) {
    return progressEvent.status;
  }

  return trackableStatuses.has(job.status as JobProgressEvent['status']) ? job.status as JobProgressEvent['status'] : 'pending';
}

function progressValue(job: VideoJob, progressEvent?: JobProgressEvent | null) {
  if (progressEvent?.progress === null) {
    return job.progress;
  }

  if (typeof progressEvent?.progress === 'number') {
    return progressPercent(progressEvent.progress);
  }

  return job.progress;
}

function formatDuration(ms?: number | null) {
  if (!Number.isFinite(ms) || ms === undefined || ms === null || ms <= 0) {
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

function currentStage(status: JobProgressEvent['status'], stage?: JobStage) {
  if (stage) {
    return stage;
  }

  if (status === 'completed') {
    return 'finalizing';
  }

  if (status === 'processing' || status === 'failed') {
    return 'detecting';
  }

  return 'preparing';
}

function stageState(stage: JobStage, activeStage: JobStage, status: JobProgressEvent['status']) {
  const activeIndex = stages.indexOf(activeStage);
  const stageIndex = stages.indexOf(stage);

  if (status === 'completed') {
    return 'done';
  }

  if (status === 'failed' && stageIndex === activeIndex) {
    return 'failed';
  }

  if (status === 'cancelled' && stageIndex === activeIndex) {
    return 'cancelled';
  }

  if (stageIndex < activeIndex) {
    return 'done';
  }

  if (stageIndex === activeIndex) {
    return 'current';
  }

  return 'upcoming';
}

export function JobTaskDrawer({
  job,
  progressEvent,
  eventCount,
  onClose,
  onRetry,
  onCancel,
  onViewEvents,
  onViewPlayback,
}: JobTaskDrawerProps) {
  const titleId = useId();
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const status = effectiveStatus(job, progressEvent);
  const statusText = jobStatusLabel(status);
  const stage = currentStage(status, progressEvent?.stage);
  const stageText = progressEvent?.stage ? jobStageLabel(progressEvent.stage) : jobActivityLabel(progressEvent, job.status);
  const eta = formatDuration(progressEvent?.estimated_remaining_ms);
  const lastUpdate = formatRecentUpdate(progressEvent?.updated_at);
  const progress = Math.max(0, Math.min(100, Math.round(progressValue(job, progressEvent))));
  const canRetry = retryableStatuses.has(status);
  const canCancel = Boolean(onCancel) && activeStatuses.has(status);
  const canViewEvents = status === 'completed' && Boolean(onViewEvents) && typeof eventCount === 'number' && eventCount > 0;
  const canViewPlayback = status === 'completed' && Boolean(onViewPlayback) && job.annotated_video_status === 'ready' && Boolean(job.annotated_video_url);

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeButtonRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
        return;
      }

      if (event.key === 'Tab') {
        const focusable = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"] button, [role="dialog"] input, [role="dialog"] select, [role="dialog"] textarea, [role="dialog"] [tabindex]:not([tabindex="-1"])'))
          .filter((element) => !element.hasAttribute('disabled'));
        if (!focusable.length) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      previousFocus?.focus();
    };
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={(event) => event.target === event.currentTarget && onClose()}>
      <div className="modal task-drawer" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <div className="task-drawer-top">
          <div>
            <p className="eyebrow">TASK DETAIL</p>
            <h3 id={titleId}>任务详情</h3>
            <p className="task-drawer-subtitle">{job.filename}</p>
          </div>
          <div className="task-drawer-top-actions">
            <span className={`job-status-badge ${status}`}>{statusText}</span>
            <button ref={closeButtonRef} type="button" className="job-action-button task-drawer-close" onClick={onClose} aria-label="关闭任务详情">
              关闭
            </button>
          </div>
        </div>

        <div className="task-drawer-meta">
          <div>
            <span>当前阶段</span>
            <strong>{stageText}</strong>
          </div>
          <div>
            <span>处理进度</span>
            <strong>{`${progress}%`}</strong>
          </div>
          {lastUpdate && (
            <div>
              <span>最近更新</span>
              <strong>{lastUpdate}</strong>
            </div>
          )}
          {eta && (
            <div>
              <span>预计剩余</span>
              <strong>{eta}</strong>
            </div>
          )}
          {typeof eventCount === 'number' && eventCount > 0 && (
            <div>
              <span>关联事件</span>
              <strong>{`${eventCount} 条`}</strong>
            </div>
          )}
        </div>

        <div className="task-drawer-progress">
          <JobProgressBar progress={progress} indeterminate={progressEvent?.progress === null} label={`${job.filename} 抽屉进度`} />
          <p className="task-drawer-message">
            {status === 'failed' ? `失败原因：${progressEvent?.message ?? '任务执行失败'}` : progressEvent?.message ?? stageText}
          </p>
        </div>

        <div className="task-drawer-stage-block">
          <span>阶段时间线</span>
          <ol className="task-stage-timeline">
            {stages.map((item) => (
              <li key={item} className={`task-stage-item ${stageState(item, stage, status)}`}>
                <span className="task-stage-dot" aria-hidden="true" />
                <div>
                  <strong>{jobStageLabel(item)}</strong>
                  <small>{item === stage ? statusText : ' '}</small>
                </div>
              </li>
            ))}
          </ol>
        </div>

        <div className="task-drawer-actions">
          {canViewEvents && (
            <button className="job-action-button" type="button" onClick={() => onViewEvents?.(job.id)}>
              查看事件
            </button>
          )}
          {canViewPlayback && (
            <button className="job-action-button" type="button" onClick={() => onViewPlayback?.(job.id)}>
              播放检测回放
            </button>
          )}
          {canRetry && (
            <button className="confirm" type="button" onClick={() => onRetry(job.id)}>
              重新处理
            </button>
          )}
          {canCancel && (
            <button className="job-action-button" type="button" onClick={() => onCancel?.(job.id)}>
              取消任务
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
