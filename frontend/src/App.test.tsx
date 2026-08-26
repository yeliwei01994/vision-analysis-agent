import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import App from './App';

afterEach(() => { cleanup(); vi.clearAllMocks(); });

const { event, apiMock } = vi.hoisted(() => {
  const event = { id: 'event-1', job_id: 'job-1', event_type: 'person_enter_zone', start_time_ms: 1000, end_time_ms: 12000, severity: 'high', status: 'unreviewed', confidence: 0.91, objects: [{ class_name: 'person', confidence: 0.94, bbox: [10, 20, 80, 160], track_id: 1 }], evidence: { frame_urls: [] }, analysis: { summary: '人员进入受限区域并持续停留', severity: 'high', suggestion: '请人工确认是否为授权人员', report_source: 'mock' }, detector_version: 'yolov8n' };
  const apiMock = {
    listEvents: vi.fn().mockResolvedValue([event]),
    listRules: vi.fn().mockResolvedValue([]),
    listJobs: vi.fn().mockResolvedValue([]),
    createVideo: vi.fn(), uploadVideo: vi.fn(), processVideo: vi.fn(), getJob: vi.fn(), updateJob: vi.fn(), deleteJob: vi.fn(), deleteEvent: vi.fn(),
    confirmEvent: vi.fn().mockResolvedValue({ ...event, status: 'confirmed' }),
    ignoreEvent: vi.fn().mockResolvedValue({ ...event, status: 'ignored' }),
  };
  return { event, apiMock };
});

vi.mock('./api/client', () => ({ api: apiMock }));

test('shows empty event state when no events exist', async () => {
  apiMock.listEvents.mockResolvedValueOnce([]);
  render(<App />);
  expect(await screen.findByText('暂无匹配事件')).toBeInTheDocument();
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
