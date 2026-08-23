import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import App from './App';

afterEach(() => { cleanup(); vi.clearAllMocks(); });

const { event, apiMock } = vi.hoisted(() => {
  const event = { id: 'event-1', job_id: 'job-1', event_type: 'person_enter_zone', start_time_ms: 1000, end_time_ms: 12000, severity: 'high', status: 'unreviewed', confidence: 0.91, objects: [{ class_name: 'person', confidence: 0.94, bbox: [10, 20, 80, 160], track_id: 1 }], evidence: { frame_urls: [] }, analysis: { summary: '人员进入受限区域并持续停留', severity: 'high', suggestion: '请人工确认是否为授权人员', report_source: 'mock' } };
  const apiMock = {
    listEvents: vi.fn().mockResolvedValue([event]),
    listRules: vi.fn().mockResolvedValue([]),
    createVideo: vi.fn(), uploadVideo: vi.fn(), processVideo: vi.fn(), getJob: vi.fn(),
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

test('confirming an event calls the review API and updates its status', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  render(<App />);
  await screen.findAllByText('person_enter_zone');

  fireEvent.click(screen.getByRole('button', { name: '确认事件' }));

  await waitFor(() => expect(apiMock.confirmEvent).toHaveBeenCalledWith('event-1'));
  expect(await screen.findByText('已确认')).toBeInTheDocument();
});

test('ignoring an event calls the review API and updates its status', async () => {
  apiMock.listEvents.mockResolvedValueOnce([event]);
  render(<App />);
  await screen.findAllByText('person_enter_zone');

  fireEvent.click(screen.getByRole('button', { name: '忽略' }));

  await waitFor(() => expect(apiMock.ignoreEvent).toHaveBeenCalledWith('event-1'));
  expect((await screen.findAllByText('已忽略')).length).toBeGreaterThan(0);
});
