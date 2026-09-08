import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { vi } from 'vitest';
import type { JobProgressEvent, VideoJob } from '../types/events';
import { JobProgressBar } from './JobProgressBar';
import { JobTaskCard } from './JobTaskCard';
import { JobsPage, RulesPage } from './WorkspacePages';

const { deleteJob, updateJob, updateRule } = vi.hoisted(() => ({
  deleteJob: vi.fn(),
  updateJob: vi.fn(),
  updateRule: vi.fn(),
}));

vi.mock('../api/client', () => ({ api: { deleteJob, updateJob, updateRule } }));

const processingJob: VideoJob = {
  id: 'job-processing',
  filename: 'processing.mp4',
  duration_ms: 125_000,
  status: 'processing',
  progress: 55,
  source_uri: null,
};

const completedJob: VideoJob = {
  id: 'job-completed',
  filename: 'completed.mp4',
  duration_ms: 63_000,
  status: 'completed',
  progress: 100,
  source_uri: null,
};

const failedJob: VideoJob = {
  id: 'job-failed',
  filename: 'failed.mp4',
  duration_ms: 92_000,
  status: 'failed',
  progress: 100,
  source_uri: null,
};

const processingProgress: JobProgressEvent = {
  job_id: 'job-processing',
  status: 'processing',
  stage: 'detecting',
  progress: 55,
  message: '已完成 120 / 240 帧',
  updated_at: '2026-08-31T08:00:00.000Z',
  estimated_remaining_ms: 61_000,
  sequence: 4,
};

const failedProgress: JobProgressEvent = {
  job_id: 'job-failed',
  status: 'failed',
  progress: 100,
  message: '模型服务暂时不可用',
  updated_at: 'invalid-timestamp',
  estimated_remaining_ms: null,
  sequence: 8,
};

test('resetting a saved zone enables applying an empty geometry', async () => {
  const rule = {
    event_type: 'person_enter_zone',
    class_name: 'person',
    min_confidence: 0.25,
    min_duration_ms: 0,
    version: 'rule-v1',
    geometry: { kind: 'polygon' as const, points: [[0.1, 0.1], [0.8, 0.1], [0.8, 0.8], [0.1, 0.8]] as [number, number][] },
    enabled: true,
  };

  render(<RulesPage rules={[rule]} events={[]} onSaved={async () => {}} />);
  fireEvent.click(screen.getByRole('button', { name: '编辑区域' }));
  fireEvent.click(screen.getByRole('button', { name: '重置' }));

  const apply = screen.getByRole('button', { name: '应用区域' });
  expect(apply).toBeEnabled();
  fireEvent.click(apply);
  fireEvent.click(screen.getByRole('button', { name: '保存规则' }));

  await waitFor(() => expect(updateRule).toHaveBeenCalledWith(expect.objectContaining({ geometry: null })));
});

test('renders translated task cards with progress details and contextual actions', () => {
  const onOpenJob = vi.fn();
  const onRetryJob = vi.fn();

  render(
    <JobsPage
      jobs={[completedJob, failedJob, processingJob]}
      progressById={{
        [processingJob.id]: processingProgress,
        [failedJob.id]: failedProgress,
      }}
      connectionState="connected"
      onOpenJob={onOpenJob}
      onRetryJob={onRetryJob}
      onRefresh={async () => {}}
    />,
  );

  expect(screen.queryByText('实时进度连接已断开，正在轮询任务状态…')).not.toBeInTheDocument();

  const cards = screen.getAllByRole('article');
  expect(cards).toHaveLength(3);
  expect(cards.map((card) => within(card).getByRole('heading', { level: 3 }).textContent)).toEqual([
    'processing.mp4',
    'failed.mp4',
    'completed.mp4',
  ]);

  const processingCard = screen.getByRole('article', { name: 'processing.mp4' });
  expect(within(processingCard).getByText('正在处理')).toBeInTheDocument();
  expect(within(processingCard).getByText('正在进行目标检测')).toBeInTheDocument();
  expect(within(processingCard).getByText('最近更新')).toBeInTheDocument();
  expect(within(processingCard).getByText('预计剩余')).toBeInTheDocument();
  expect(within(processingCard).getByText('已完成 120 / 240 帧')).toBeInTheDocument();
  expect(within(processingCard).getByRole('progressbar', { name: 'processing.mp4 进度' })).toHaveAttribute('aria-valuenow', '55');
  expect(within(processingCard).getByRole('button', { name: '打开 processing.mp4' })).toBeInTheDocument();
  expect(within(processingCard).getByRole('button', { name: '编辑 processing.mp4' })).toBeInTheDocument();
  expect(within(processingCard).getByRole('button', { name: '删除 processing.mp4' })).toBeDisabled();
  expect(within(processingCard).queryByRole('button', { name: '重试 processing.mp4' })).not.toBeInTheDocument();

  const failedCard = screen.getByRole('article', { name: 'failed.mp4' });
  expect(within(failedCard).getAllByText('处理失败').length).toBeGreaterThan(0);
  expect(within(failedCard).queryByText('最近更新')).not.toBeInTheDocument();
  expect(within(failedCard).queryByText('预计剩余')).not.toBeInTheDocument();
  fireEvent.click(within(failedCard).getByRole('button', { name: '重试 failed.mp4' }));
  fireEvent.click(within(failedCard).getByRole('button', { name: '打开 failed.mp4' }));
  expect(onRetryJob).toHaveBeenCalledWith('job-failed');
  expect(onOpenJob).toHaveBeenCalledWith('job-failed');
});

test('shows reconnecting notice only while reconnecting', () => {
  const props = {
    jobs: [processingJob],
    progressById: { [processingJob.id]: processingProgress },
    onOpenJob: vi.fn(),
    onRetryJob: vi.fn(),
    onRefresh: async () => {},
  };

  const { rerender } = render(<JobsPage {...props} connectionState="connected" />);
  expect(screen.queryByText('实时进度连接已断开，正在轮询任务状态…')).not.toBeInTheDocument();

  rerender(<JobsPage {...props} connectionState="reconnecting" />);
  expect(screen.getByText('实时进度连接已断开，正在轮询任务状态…')).toBeInTheDocument();

  rerender(<JobsPage {...props} connectionState="offline" />);
  expect(screen.queryByText('实时进度连接已断开，正在轮询任务状态…')).not.toBeInTheDocument();
});

test('omits aria-valuenow for indeterminate progress and only shows cancel when available', () => {
  const job = { ...processingJob, id: 'job-pending', filename: 'pending.mp4', status: 'pending', progress: 0 };
  const progressEvent: JobProgressEvent = {
    job_id: 'job-pending',
    status: 'pending',
    stage: 'reading',
    progress: null,
    updated_at: undefined,
    estimated_remaining_ms: null,
    sequence: 1,
  };

  const { rerender } = render(
    <JobTaskCard
      job={job}
      progressEvent={progressEvent}
      onOpen={vi.fn()}
      onRetry={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
    />,
  );

  const indeterminateBar = screen.getByRole('progressbar', { name: 'pending.mp4 进度' });
  expect(indeterminateBar).not.toHaveAttribute('aria-valuenow');
  expect(screen.queryByText('最近更新')).not.toBeInTheDocument();
  expect(screen.queryByText('预计剩余')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '取消 pending.mp4' })).not.toBeInTheDocument();

  rerender(
    <JobTaskCard
      job={job}
      progressEvent={progressEvent}
      onOpen={vi.fn()}
      onRetry={vi.fn()}
      onCancel={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
    />,
  );

  expect(screen.getByRole('button', { name: '取消 pending.mp4' })).toBeInTheDocument();
});

test('preserves edit and delete flows for completed tasks', async () => {
  updateJob.mockResolvedValueOnce({ ...completedJob, filename: 'renamed.mp4' });
  deleteJob.mockResolvedValueOnce(undefined);
  const onRefresh = vi
    .fn<() => Promise<void>>()
    .mockResolvedValueOnce(undefined)
    .mockResolvedValueOnce(undefined);

  render(
    <JobsPage
      jobs={[completedJob]}
      progressById={{}}
      connectionState="connected"
      onOpenJob={vi.fn()}
      onRetryJob={vi.fn()}
      onRefresh={onRefresh}
    />,
  );

  fireEvent.click(screen.getByRole('button', { name: '编辑 completed.mp4' }));
  fireEvent.change(screen.getByRole('textbox', { name: '任务文件名' }), { target: { value: 'renamed.mp4' } });
  fireEvent.click(screen.getByRole('button', { name: '保存修改' }));

  await waitFor(() => expect(updateJob).toHaveBeenCalledWith('job-completed', 'renamed.mp4'));

  fireEvent.click(screen.getByRole('button', { name: '删除 completed.mp4' }));
  fireEvent.click(screen.getByRole('button', { name: '确认删除' }));

  await waitFor(() => expect(deleteJob).toHaveBeenCalledWith('job-completed'));
  expect(onRefresh).toHaveBeenCalledTimes(2);
});

test('uses a shared action-button contract for accessible target sizing and motion overrides', () => {
  render(
    <JobTaskCard
      job={processingJob}
      progressEvent={processingProgress}
      onOpen={vi.fn()}
      onRetry={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
    />,
  );

  const actions = screen.getAllByRole('button');
  expect(actions.length).toBeGreaterThan(0);
  for (const action of actions) {
    expect(action).toHaveClass('job-action-button');
  }
});

test('exposes determinate progressbar semantics when progress is known', () => {
  render(<JobProgressBar progress={72} label="known progress" />);

  expect(screen.getByRole('progressbar', { name: 'known progress' })).toHaveAttribute('aria-valuenow', '72');
});

test('renders backend progress value 1 as one percent', () => {
  render(<JobTaskCard job={{ ...processingJob, progress: 1 }} progressEvent={{ ...processingProgress, progress: 1 }} onOpen={vi.fn()} onRetry={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />);

  expect(screen.getByRole('progressbar', { name: 'processing.mp4 进度' })).toHaveAttribute('aria-valuenow', '1');
  expect(screen.getByText('1%')).toBeInTheDocument();
});
