import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, vi } from 'vitest';
import App from './App';

afterEach(() => { cleanup(); vi.useRealTimers(); });

vi.spyOn(window, 'open').mockImplementation(() => null);

const { event, apiMock, progressMock } = vi.hoisted(() => {
  const event = { id: 'event-1', job_id: 'job-1', event_type: 'person_enter_zone', start_time_ms: 1000, end_time_ms: 12000, severity: 'high', status: 'unreviewed', confidence: 0.91, objects: [{ class_name: 'person', confidence: 0.94, bbox: [10, 20, 80, 160], track_id: 1 }], evidence: { frame_urls: [] }, analysis: { summary: '人员进入受限区域并持续停留', severity: 'high', suggestion: '请人工确认是否为授权人员', report_source: 'mock' }, detector_version: 'yolov8n' };
  const progressMock = {
    onEvent: undefined as undefined | ((event: Record<string, unknown>) => void),
    onStateChange: undefined as undefined | ((state: 'connected' | 'reconnecting') => void),
    unsubscribe: vi.fn(),
  };
  const apiMock = {
    listEvents: vi.fn().mockResolvedValue([event]),
    listRules: vi.fn().mockResolvedValue([]),
    listJobs: vi.fn().mockResolvedValue([]),
    createVideo: vi.fn(), uploadVideo: vi.fn(), processVideo: vi.fn(), getJob: vi.fn(), updateJob: vi.fn(), deleteJob: vi.fn(), deleteEvent: vi.fn(),
    confirmEvent: vi.fn().mockResolvedValue({ ...event, status: 'confirmed' }),
    ignoreEvent: vi.fn().mockResolvedValue({ ...event, status: 'ignored' }),
    listReviews: vi.fn().mockResolvedValue([]),
    reviewEvent: vi.fn().mockResolvedValue(event),
    queryEvents: vi.fn().mockResolvedValue({ items: [], total: 0, page: 1, page_size: 1 }),
    subscribeJobProgress: vi.fn((onEvent, onStateChange) => {
      progressMock.onEvent = onEvent;
      progressMock.onStateChange = onStateChange;
      return progressMock.unsubscribe;
    }),
  };
  return { event, apiMock, progressMock };
});

vi.mock('./api/client', () => ({ api: apiMock }));

beforeEach(() => {
  apiMock.listEvents.mockReset().mockResolvedValue([event]);
  apiMock.listRules.mockReset().mockResolvedValue([]);
  apiMock.listJobs.mockReset().mockResolvedValue([]);
  apiMock.createVideo.mockReset();
  apiMock.uploadVideo.mockReset();
  apiMock.processVideo.mockReset();
  apiMock.getJob.mockReset();
  apiMock.updateJob.mockReset();
  apiMock.deleteJob.mockReset();
  apiMock.deleteEvent.mockReset();
  apiMock.confirmEvent.mockReset().mockResolvedValue({ ...event, status: 'confirmed' });
  apiMock.ignoreEvent.mockReset().mockResolvedValue({ ...event, status: 'ignored' });
  apiMock.listReviews.mockReset().mockResolvedValue([]);
  apiMock.reviewEvent.mockReset().mockResolvedValue(event);
  apiMock.queryEvents.mockReset().mockResolvedValue({ items: [], total: 0, page: 1, page_size: 1 });
  apiMock.subscribeJobProgress.mockReset().mockImplementation((onEvent, onStateChange) => {
    progressMock.onEvent = onEvent;
    progressMock.onStateChange = onStateChange;
    return progressMock.unsubscribe;
  });
  progressMock.unsubscribe.mockReset();
  progressMock.onEvent = undefined;
  progressMock.onStateChange = undefined;
});

test('shows a persistent task summary bar that opens and closes a keyboard-friendly drawer', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 125_000, status: 'processing', progress: 42, source_uri: '/media/clip.mp4' }]);

  render(<App />);
  await waitFor(() => expect(apiMock.listJobs).toHaveBeenCalledTimes(1));

  act(() => {
    progressMock.onEvent?.({
      job_id: 'job-1',
      status: 'processing',
      stage: 'detecting',
      progress: 42,
      message: '已完成 120 / 240 帧',
      updated_at: '2026-08-31T08:00:00.000Z',
      estimated_remaining_ms: 61_000,
      sequence: 2,
    });
  });

  const summary = screen.getByRole('button', { name: /clip\.mp4/ });
  expect(summary).toHaveTextContent('正在进行目标检测');
  fireEvent.click(summary);

  const drawer = screen.getByRole('dialog', { name: '任务详情' });
  expect(drawer).toBeInTheDocument();
  expect(within(drawer).getByRole('button', { name: '关闭任务详情' })).toBeInTheDocument();
  expect(within(drawer).getByText('已完成 120 / 240 帧')).toBeInTheDocument();
  expect(within(drawer).getByText('预计剩余')).toBeInTheDocument();

  fireEvent.keyDown(document, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog', { name: '任务详情' })).not.toBeInTheDocument());
});

test('shows completion result actions from shared task state and keeps event review and playback flows intact', async () => {
  const completedEvent = {
    ...event,
    evidence: {
      frame_urls: ['/media/evidence/event-1/frame-1.jpg'],
      frames: [{ timestamp_ms: 0, image_url: '/media/evidence/event-1/frame-1.jpg', detections: event.objects }],
    },
  };
  apiMock.listEvents
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([completedEvent]);
  apiMock.listJobs
    .mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 6_000, status: 'processing', progress: 88, source_uri: '/media/clip.mp4', annotated_video_status: 'pending', annotated_video_url: null }])
    .mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: '/media/clip.mp4', annotated_video_status: 'ready', annotated_video_url: '/media/annotated/job-1.mp4' }]);
  apiMock.queryEvents.mockResolvedValue({ items: [completedEvent], total: 1, page: 1, page_size: 1 });

  render(<App />);
  await waitFor(() => expect(apiMock.listJobs).toHaveBeenCalledTimes(1));

  act(() => {
    progressMock.onEvent?.({
      job_id: 'job-1',
      status: 'completed',
      stage: 'finalizing',
      progress: 100,
      message: '已生成事件与检测回放',
      updated_at: '2026-08-31T08:03:00.000Z',
      estimated_remaining_ms: 0,
      sequence: 3,
    });
  });

  await waitFor(() => expect(screen.getByRole('button', { name: '查看任务详情 clip.mp4' })).toHaveTextContent('关联事件 1 条'));

  fireEvent.click(screen.getByRole('button', { name: /clip\.mp4/ }));

  const drawer = screen.getByRole('dialog', { name: '任务详情' });
  expect(drawer).toBeInTheDocument();
  expect(within(drawer).getByText('已生成事件与检测回放')).toBeInTheDocument();
  expect(within(drawer).getByText('关联事件')).toBeInTheDocument();
  expect(within(drawer).getByText('1 条')).toBeInTheDocument();
  expect(within(drawer).getByRole('button', { name: '查看事件' })).toBeInTheDocument();
  expect(within(drawer).getByRole('button', { name: '播放检测回放' })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: '查看事件' }));
  expect(await screen.findByRole('heading', { name: '事件详情' })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: /clip\.mp4/ }));
  fireEvent.click(screen.getByRole('button', { name: '播放检测回放' }));
  expect(await screen.findByLabelText('YOLO 检测回放')).toBeInTheDocument();
});

test('loads a server-backed task result so playback works even when no related event is in the bounded cache', async () => {
  const uncachedEvent = {
    ...event,
    evidence: {
      frame_urls: ['/media/evidence/event-1/frame-1.jpg'],
      frames: [{ timestamp_ms: 0, image_url: '/media/evidence/event-1/frame-1.jpg', detections: event.objects }],
    },
  };
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([
    {
      id: 'job-1',
      filename: 'clip.mp4',
      duration_ms: 6_000,
      status: 'completed',
      progress: 100,
      source_uri: '/media/clip.mp4',
      annotated_video_status: 'ready',
      annotated_video_url: '/media/annotated/job-1.mp4',
    },
  ]);
  apiMock.queryEvents.mockResolvedValue({ items: [uncachedEvent], total: 1, page: 1, page_size: 1 });

  render(<App />);

  fireEvent.click(await screen.findByRole('button', { name: '查看任务详情 clip.mp4' }));
  const drawer = await screen.findByRole('dialog', { name: '任务详情' });
  expect(await within(drawer).findByRole('button', { name: '播放检测回放' })).toBeInTheDocument();

  fireEvent.click(within(drawer).getByRole('button', { name: '播放检测回放' }));

  expect(await screen.findByRole('heading', { name: '事件详情' })).toBeInTheDocument();
  expect(await screen.findByLabelText('YOLO 检测回放')).toBeInTheDocument();
});

test('uses the server-backed result count for completed jobs beyond the bounded event list', async () => {
  const uncachedEvent = {
    ...event,
    evidence: {
      frame_urls: ['/media/evidence/event-1/frame-1.jpg'],
      frames: [{ timestamp_ms: 0, image_url: '/media/evidence/event-1/frame-1.jpg', detections: event.objects }],
    },
  };
  const boundedEvents = Array.from({ length: 50 }, (_, index) => ({
    ...event,
    id: `event-bounded-${index}`,
    job_id: `job-bounded-${index}`,
    event_type: 'person_stay',
  }));
  apiMock.listEvents.mockResolvedValueOnce(boundedEvents);
  apiMock.listJobs.mockResolvedValueOnce([
    {
      id: 'job-1',
      filename: 'clip.mp4',
      duration_ms: 6_000,
      status: 'completed',
      progress: 100,
      source_uri: '/media/clip.mp4',
      annotated_video_status: 'failed',
      annotated_video_url: null,
    },
  ]);
  apiMock.queryEvents.mockResolvedValue({ items: [uncachedEvent], total: 3, page: 1, page_size: 1 });

  render(<App />);

  fireEvent.click(await screen.findByRole('button', { name: '查看任务详情 clip.mp4' }));
  const drawer = await screen.findByRole('dialog', { name: '任务详情' });

  expect(await within(drawer).findByText('关联事件')).toBeInTheDocument();
  expect(within(drawer).getByText('3 条')).toBeInTheDocument();
  expect(within(drawer).getByRole('button', { name: '查看事件' })).toBeInTheDocument();

  fireEvent.click(within(drawer).getByRole('button', { name: '查看事件' }));
  expect(await screen.findByRole('heading', { name: '事件详情' })).toBeInTheDocument();
});

test('shows failure details and lets the operator retry from the task drawer', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 4_000, status: 'processing', progress: 76, source_uri: '/media/clip.mp4' }]);
  apiMock.processVideo.mockResolvedValueOnce({ id: 'job-1', filename: 'clip.mp4', duration_ms: 4_000, status: 'pending', progress: 0, source_uri: '/media/clip.mp4' });

  render(<App />);
  await waitFor(() => expect(apiMock.listJobs).toHaveBeenCalledTimes(1));

  act(() => {
    progressMock.onEvent?.({
      job_id: 'job-1',
      status: 'failed',
      stage: 'analyzing_events',
      progress: 76,
      message: '模型服务暂时不可用',
      updated_at: '2026-08-31T08:05:00.000Z',
      estimated_remaining_ms: null,
      sequence: 4,
    });
  });

  fireEvent.click(screen.getByRole('button', { name: /clip\.mp4/ }));

  const drawer = screen.getByRole('dialog', { name: '任务详情' });
  expect(drawer).toBeInTheDocument();
  expect(within(drawer).getAllByText('处理失败').length).toBeGreaterThan(0);
  expect(within(drawer).getByText('失败原因：模型服务暂时不可用')).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: '重新处理' }));
  await waitFor(() => expect(apiMock.processVideo).toHaveBeenCalledWith('job-1'));
});

test('shows empty event state when no events exist', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  render(<App />);
  expect(await screen.findByText('暂无匹配事件')).toBeInTheDocument();
});

test('loads a bounded event list after initialization', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  render(<App />);
  await waitFor(() => expect(apiMock.listEvents).toHaveBeenCalledWith(50));
});

test('keeps the original metrics and upload progress panel', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  render(<App />);
  expect(await screen.findByText('今日事件')).toBeInTheDocument();
  expect(screen.getByText('待复核')).toBeInTheDocument();
  expect(screen.getByText('处理任务')).toBeInTheDocument();
  expect(screen.getByText('系统状态')).toBeInTheDocument();
  expect(screen.getByText('把视频里的异常，变成可检索、可复核的业务事件。')).toBeInTheDocument();
});

test('upload control sends the selected video to the upload API', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.uploadVideo.mockResolvedValueOnce({ id: 'job-upload', filename: 'clip.mp4', duration_ms: 0, status: 'pending', progress: 0 });
  apiMock.processVideo.mockResolvedValueOnce({ id: 'job-upload', filename: 'clip.mp4', duration_ms: 0, status: 'processing', progress: 1 });
  render(<App />);
  const file = new File(['video'], 'clip.mp4', { type: 'video/mp4' });
  fireEvent.change(screen.getByLabelText('导入视频任务'), { target: { files: [file] } });
  await waitFor(() => expect(apiMock.uploadVideo).toHaveBeenCalledWith(file));
});

test('starts polling after a reconnecting signal and stops once the stream reconnects', async () => {
  vi.useFakeTimers();
  apiMock.listEvents.mockResolvedValue([]);
  apiMock.listJobs
    .mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 0, status: 'processing', progress: 20, source_uri: null }])
    .mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 0, status: 'completed', progress: 100, source_uri: null }])
    .mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 0, status: 'completed', progress: 100, source_uri: null }]);

  render(<App />);
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  expect(apiMock.listJobs).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByRole('button', { name: '视频任务' }));
  expect(screen.getByText('20%')).toBeInTheDocument();

  act(() => {
    progressMock.onEvent?.({
      job_id: 'job-1',
      status: 'processing',
      stage: 'detecting',
      progress: 20,
      sequence: 2,
      updated_at: '2026-08-31T08:00:00.000Z',
    });
  });

  act(() => {
    progressMock.onStateChange?.('reconnecting');
  });

  expect(screen.getByText('实时重连中')).toBeInTheDocument();
  expect(screen.getByText('实时进度连接已断开，正在轮询任务状态…')).toBeInTheDocument();

  await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
  expect(apiMock.listJobs).toHaveBeenCalledTimes(3);
  expect(screen.getByText('100%')).toBeInTheDocument();
  expect(screen.getAllByText('处理完成').length).toBeGreaterThan(0);

  act(() => {
    progressMock.onStateChange?.('connected');
  });

  await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
  expect(apiMock.listJobs).toHaveBeenCalledTimes(3);
});

test('releases the upload control while background processing continues', async () => {
  apiMock.listEvents.mockResolvedValue([]);
  apiMock.listJobs.mockResolvedValue([]);
  apiMock.uploadVideo.mockResolvedValueOnce({ id: 'job-background', filename: 'clip.mp4', duration_ms: 0, status: 'pending', progress: 0 });
  apiMock.processVideo.mockResolvedValueOnce({ id: 'job-background', filename: 'clip.mp4', duration_ms: 0, status: 'processing', progress: 1 });
  apiMock.getJob.mockReturnValueOnce(new Promise(() => {}));

  render(<App />);
  fireEvent.change(screen.getByLabelText('导入视频任务'), {
    target: { files: [new File(['video'], 'clip.mp4', { type: 'video/mp4' })] },
  });

  await waitFor(() => expect(apiMock.processVideo).toHaveBeenCalledWith('job-background'));
  expect(screen.getByLabelText('导入视频任务')).not.toBeDisabled();
});

test('shows an uploaded job in the shared task list immediately after acceptance', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([]);
  apiMock.uploadVideo.mockResolvedValueOnce({ id: 'job-uploaded', filename: 'clip.mp4', duration_ms: 2_000, status: 'pending', progress: 0, source_uri: null });
  apiMock.processVideo.mockResolvedValueOnce({ id: 'job-uploaded', filename: 'clip.mp4', duration_ms: 2_000, status: 'processing', progress: 1, source_uri: null });
  apiMock.getJob.mockReturnValueOnce(new Promise(() => {}));

  render(<App />);
  fireEvent.change(screen.getByLabelText('导入视频任务'), {
    target: { files: [new File(['video'], 'clip.mp4', { type: 'video/mp4' })] },
  });

  await waitFor(() => expect(apiMock.uploadVideo).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(apiMock.processVideo).toHaveBeenCalledWith('job-uploaded'));

  fireEvent.click(screen.getByRole('button', { name: '视频任务' }));
  expect((await screen.findAllByRole('article', { name: 'clip.mp4' })).length).toBeGreaterThan(0);
});

test('merges realtime job progress updates into the app-level task list', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 2_000, status: 'processing', progress: 10, source_uri: null }]);

  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: '视频任务' }));

  expect(await screen.findByText('clip.mp4')).toBeInTheDocument();
  expect(apiMock.subscribeJobProgress).toHaveBeenCalledTimes(1);

  act(() => {
    progressMock.onEvent?.({
      job_id: 'job-1',
      status: 'processing',
      stage: 'detecting',
      progress: 55,
      sequence: 2,
      updated_at: '2026-08-31T08:00:00.000Z',
    });
  });

  expect(await screen.findByText('55%')).toBeInTheDocument();
});

test('shows real evidence frames and lets the reviewer select a timeline point', async () => {
  const evidenced = { ...event, event_type: 'person_stay', rule_version: 'rule-v1', analysis: null, evidence: { frame_urls: ['/media/evidence/event-1/frame-1.jpg', '/media/evidence/event-1/frame-2.jpg'], frames: [
    { timestamp_ms: 0, image_url: '/media/evidence/event-1/frame-1.jpg', detections: event.objects },
    { timestamp_ms: 500, image_url: '/media/evidence/event-1/frame-2.jpg', detections: event.objects },
  ] } };
  apiMock.listEvents.mockResolvedValueOnce([evidenced]);
  render(<App />);

  expect((await screen.findAllByText('人员停留')).length).toBeGreaterThan(0);
  expect((await screen.findAllByText('人员 1 次检测 · 平均置信度 94%')).length).toBeGreaterThan(0);
  fireEvent.click(screen.getByRole('button', { name: '证据帧 00:00.5' }));
  expect(screen.getByRole('img', { name: '00:00.5 的检测证据' })).toHaveAttribute('src', '/media/evidence/event-1/frame-2.jpg');
});

test('shows a retry control when an evidence image cannot be loaded', async () => {
  const evidenced = { ...event, evidence: { frame_urls: ['/media/evidence/event-1/frame-1.jpg'], frames: [
    { timestamp_ms: 0, image_url: '/media/evidence/event-1/frame-1.jpg', detections: event.objects },
  ] } };
  apiMock.listEvents.mockResolvedValueOnce([evidenced]);
  render(<App />);

  fireEvent.error(await screen.findByRole('img', { name: '00:00.0 的检测证据' }));
  expect(await screen.findByText('证据文件不可用')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '重新加载证据图片' }));
  expect(screen.getByRole('img', { name: '00:00.0 的检测证据' })).toBeInTheDocument();
});

test('confirming an event calls the review API and updates its status', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  render(<App />);
  await screen.findAllByText('person_enter_zone');

  fireEvent.click(screen.getByRole('button', { name: '确认事件' }));

  await waitFor(() => expect(apiMock.confirmEvent).toHaveBeenCalledWith('event-1'));
  expect(await screen.findByText('已确认')).toBeInTheDocument();
  expect(screen.getByText('检测摘要')).toBeInTheDocument();
  expect(screen.getByText('人员 1 次检测 · 平均置信度 94%')).toBeInTheDocument();
  expect(screen.getByText(/检测器：yolov8n/)).toBeInTheDocument();
});

test('ignoring an event calls the review API and updates its status', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  render(<App />);
  await screen.findAllByText('person_enter_zone');

  fireEvent.click(screen.getByRole('button', { name: '忽略' }));

  await waitFor(() => expect(apiMock.ignoreEvent).toHaveBeenCalledWith('event-1'));
  expect((await screen.findAllByText('已忽略')).length).toBeGreaterThan(0);
});

test('deleting an event removes it from the event stream after confirmation', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  apiMock.deleteEvent.mockResolvedValueOnce(undefined);
  render(<App />);
  await screen.findAllByText('person_enter_zone');

  fireEvent.click(screen.getByRole('button', { name: '删除事件 person_enter_zone' }));
  expect(screen.getByRole('dialog', { name: '确认删除事件？' })).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '确认删除' }));

  await waitFor(() => expect(apiMock.deleteEvent).toHaveBeenCalledWith('event-1'));
  expect(await screen.findByText('暂无匹配事件')).toBeInTheDocument();
});

test('navigation opens the video tasks, rules, and model pages', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  render(<App />);

  fireEvent.click(screen.getByRole('button', { name: '视频任务' }));
  expect(await screen.findByRole('heading', { name: '视频任务' })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: '规则配置' }));
  expect(await screen.findByRole('heading', { name: '规则配置' })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: '模型版本' }));
  expect(await screen.findByRole('heading', { name: '模型版本' })).toBeInTheDocument();
});

test('can rename and delete a completed video job', async () => {
  const job = { id: 'job-1', filename: 'before.mp4', duration_ms: 1000, status: 'completed', progress: 100, source_uri: null };
  apiMock.listEvents.mockResolvedValueOnce([]);
  apiMock.listJobs.mockResolvedValueOnce([job]);
  apiMock.listJobs.mockResolvedValueOnce([{ ...job, filename: 'after.mp4' }]);
  apiMock.updateJob.mockResolvedValueOnce({ ...job, filename: 'after.mp4' });
  apiMock.deleteJob.mockResolvedValueOnce(undefined);
  render(<App />);

  fireEvent.click(screen.getByRole('button', { name: '视频任务' }));
  expect(await screen.findByText('before.mp4')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '编辑 before.mp4' }));
  const input = screen.getByRole('textbox', { name: '任务文件名' });
  fireEvent.change(input, { target: { value: 'after.mp4' } });
  fireEvent.click(screen.getByRole('button', { name: '保存修改' }));
  await waitFor(() => expect(apiMock.updateJob).toHaveBeenCalledWith('job-1', 'after.mp4'));

  fireEvent.click(screen.getByRole('button', { name: '删除 after.mp4' }));
  fireEvent.click(screen.getByRole('button', { name: '确认删除' }));
  await waitFor(() => expect(apiMock.deleteJob).toHaveBeenCalledWith('job-1'));
});

test('shows original and YOLO playback choices for the selected event job', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  apiMock.listJobs.mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: '/media/clip.mp4', annotated_video_url: '/media/annotated/job-1.mp4', annotated_video_status: 'ready', annotated_video_error: null }]);
  render(<App />);

  expect(await screen.findByRole('button', { name: 'YOLO 检测回放' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '原始视频' })).toBeInTheDocument();
  const originalVideo = screen.getByLabelText('原始视频');
  expect(screen.getByTestId('playback-source')).toHaveAttribute('src', '/media/clip.mp4');
  fireEvent.click(screen.getByRole('button', { name: 'YOLO 检测回放' }));
  expect(screen.getByTestId('playback-source')).toHaveAttribute('src', '/media/annotated/job-1.mp4');
  expect(screen.getByLabelText('YOLO 检测回放')).not.toBe(originalVideo);
  fireEvent.click(screen.getByRole('button', { name: '原始视频' }));
  expect(screen.getByTestId('playback-source')).toHaveAttribute('src', '/media/clip.mp4');
});

test('shows playback generation failure without hiding event evidence', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  apiMock.listJobs.mockResolvedValueOnce([{ id: 'job-1', filename: 'clip.mp4', duration_ms: 6_000, status: 'completed', progress: 100, source_uri: '/media/clip.mp4', annotated_video_url: null, annotated_video_status: 'failed', annotated_video_error: 'ffmpeg 不可用' }]);
  render(<App />);

  expect(await screen.findByText('检测回放生成失败：ffmpeg 不可用')).toBeInTheDocument();
  expect(screen.getByText('暂无可用抽帧证据')).toBeInTheDocument();
});
