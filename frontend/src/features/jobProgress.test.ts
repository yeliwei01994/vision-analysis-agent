import { describe, expect, it } from 'vitest';
import {
  applyJobProgressToJob,
  estimateRemainingMs,
  jobActivityLabel,
  jobStageLabel,
  jobStatusLabel,
  mergeJobProgress,
  mergeJobsSnapshot,
} from './jobProgress';
import type { JobProgressEvent, VideoJob } from '../types/events';

const baseEvent: JobProgressEvent = {
  job_id: 'job-1',
  status: 'processing',
  stage: 'reading',
  progress: 0.25,
  sequence: 2,
};

const baseJob: VideoJob = {
  id: 'job-1',
  filename: 'clip.mp4',
  duration_ms: 12_000,
  status: 'pending',
  progress: 0,
  source_uri: null,
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

  it('returns null ETA when the current sample has no numeric progress', () => {
    const history: JobProgressEvent[] = [
      { ...baseEvent, progress: 0.25, sequence: 1, updated_at: '2026-08-31T08:00:00.000Z' },
      { ...baseEvent, progress: 0.5, sequence: 2, updated_at: '2026-08-31T08:00:10.000Z' },
    ];
    const current: JobProgressEvent = { ...baseEvent, progress: null, sequence: 3, updated_at: '2026-08-31T08:00:20.000Z' };

    expect(estimateRemainingMs(history, current)).toBeNull();
  });

  it('accepts numeric updated_at values from realtime payloads', () => {
    const history: JobProgressEvent[] = [
      { ...baseEvent, progress: 0.25, sequence: 1, updated_at: 1_725_091_200_000 },
    ];
    const current: JobProgressEvent = { ...baseEvent, progress: 0.5, sequence: 2, updated_at: 1_725_091_210_000 };

    expect(estimateRemainingMs(history, current)).toBe(20000);
  });

  it('accepts percent-style progress values from backend realtime payloads', () => {
    const history: JobProgressEvent[] = [
      { ...baseEvent, progress: 25, sequence: 1, updated_at: 1_725_091_200_000 },
    ];
    const current: JobProgressEvent = { ...baseEvent, progress: 50, sequence: 2, updated_at: 1_725_091_210_000 };

    expect(estimateRemainingMs(history, current)).toBe(20000);
  });

  it('applies realtime progress onto a job record without losing its metadata', () => {
    const next = applyJobProgressToJob(baseJob, {
      ...baseEvent,
      stage: 'detecting',
      progress: 35,
      updated_at: '2026-08-31T08:00:00.000Z',
    });

    expect(next).toEqual({
      ...baseJob,
      status: 'processing',
      progress: 35,
    });
  });

  it('preserves the active in-flight job when a stale snapshot does not include it yet', () => {
    const previous = {
      'job-1': { ...baseJob, status: 'processing', progress: 10 },
      'job-2': { id: 'job-2', filename: 'older.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: null },
    };

    expect(
      mergeJobsSnapshot(
        previous,
        [{ id: 'job-2', filename: 'older.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: null }],
        { 'job-1': { ...baseEvent, progress: 35, stage: 'detecting', sequence: 3 } },
        'job-1',
      ),
    ).toEqual({
      'job-2': { id: 'job-2', filename: 'older.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: null },
      'job-1': { ...baseJob, status: 'processing', progress: 35 },
    });
  });

  it('prefers stage copy for active jobs and falls back to status labels', () => {
    expect(jobActivityLabel({ ...baseEvent, stage: 'detecting' })).toBe('正在进行目标检测');
    expect(jobActivityLabel({ ...baseEvent, stage: undefined, status: 'failed' })).toBe('处理失败');
    expect(jobActivityLabel()).toBe('等待导入');
  });

  it('maps every stage and status to user-facing labels', () => {
    expect(jobStageLabel('preparing')).toBe('正在准备视频');
    expect(jobStageLabel('reading')).toBe('正在读取视频');
    expect(jobStageLabel('extracting_frames')).toBe('正在抽取关键帧');
    expect(jobStageLabel('detecting')).toBe('正在进行目标检测');
    expect(jobStageLabel('analyzing_events')).toBe('正在分析事件');
    expect(jobStageLabel('generating_playback')).toBe('正在生成检测回放');
    expect(jobStageLabel('finalizing')).toBe('正在整理分析结果');

    expect(jobStatusLabel('pending')).toBe('等待处理');
    expect(jobStatusLabel('processing')).toBe('正在处理');
    expect(jobStatusLabel('completed')).toBe('处理完成');
    expect(jobStatusLabel('failed')).toBe('处理失败');
    expect(jobStatusLabel('cancelled')).toBe('已取消');
  });
});
