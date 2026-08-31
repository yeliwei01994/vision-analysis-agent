import { describe, expect, it } from 'vitest';
import { estimateRemainingMs, jobStageLabel, jobStatusLabel, mergeJobProgress } from './jobProgress';
import type { JobProgressEvent } from '../types/events';

const baseEvent: JobProgressEvent = {
  job_id: 'job-1',
  status: 'processing',
  stage: 'reading',
  progress: 0.25,
  sequence: 2,
};

describe('job progress', () => {
  it('accepts a newer event for the same job', () => {
    const next: JobProgressEvent = { ...baseEvent, stage: 'extracting_frames', progress: 0.5, sequence: 3 };

    expect(mergeJobProgress(baseEvent, next)).toEqual(next);
  });

  it('keeps the current event when the next event is older or from another job', () => {
    const older: JobProgressEvent = { ...baseEvent, stage: 'detecting', progress: 0.1, sequence: 1 };
    const otherJob: JobProgressEvent = { ...baseEvent, job_id: 'job-2', stage: 'detecting', progress: 0.1, sequence: 4 };

    expect(mergeJobProgress(baseEvent, older)).toEqual(baseEvent);
    expect(mergeJobProgress(baseEvent, otherJob)).toEqual(baseEvent);
  });

  it('prevents a terminal job from regressing back to processing', () => {
    const completed: JobProgressEvent = { ...baseEvent, status: 'completed', progress: 1, sequence: 5 };
    const regressing: JobProgressEvent = { ...completed, status: 'processing', progress: 0.8, sequence: 6 };

    expect(mergeJobProgress(completed, regressing)).toEqual(completed);
  });

  it('returns null ETA without enough timestamped history', () => {
    expect(estimateRemainingMs([{ ...baseEvent, updated_at: '2026-08-31T08:00:00.000Z' }], baseEvent)).toBeNull();
  });

  it('extrapolates a remaining time from two increasing samples', () => {
    const history: JobProgressEvent[] = [
      { ...baseEvent, progress: 0.25, sequence: 1, updated_at: '2026-08-31T08:00:00.000Z' },
    ];
    const current: JobProgressEvent = { ...baseEvent, progress: 0.5, sequence: 2, updated_at: '2026-08-31T08:00:10.000Z' };

    expect(estimateRemainingMs(history, current)).toBe(20000);
  });

  it('maps stages and statuses to user-facing labels', () => {
    expect(jobStageLabel('preparing')).toBe('正在准备视频');
    expect(jobStatusLabel('pending')).toBe('等待处理');
  });
});
