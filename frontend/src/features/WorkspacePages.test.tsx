import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { RulesPage } from './WorkspacePages';

const { updateRule } = vi.hoisted(() => ({ updateRule: vi.fn() }));

vi.mock('../api/client', () => ({ api: { updateRule } }));

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
