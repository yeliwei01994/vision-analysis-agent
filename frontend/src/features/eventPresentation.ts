import type { AnalysisResult, EventItem } from '../types/events';

const ruleNames: Record<string, string> = { person_stay: '人员停留', person_enter_zone: '人员进入区域' };
const classNames: Record<string, string> = { person: '人员', boat: '船只', bird: '鸟类' };
export const displayEventType = (value: string) => ruleNames[value] ?? value;
export const displayClassName = (value: string) => classNames[value] ?? value;
export function preciseTime(ms: number) { const seconds = ms / 1000; return `${Math.floor(seconds / 60).toString().padStart(2, '0')}:${(seconds % 60).toFixed(1).padStart(4, '0')}`; }
export function detectionSummary(event: EventItem) {
  const items = event.objects;
  const average = items.length ? items.reduce((total, item) => total + item.confidence, 0) / items.length : event.confidence;
  return `${displayClassName(items[0]?.class_name ?? 'unknown')} ${items.length} 次检测 · 平均置信度 ${(average * 100).toFixed(0)}%`;
}
export function fallbackAnalysis(event: EventItem): AnalysisResult {
  const duration = Math.max(1, Math.round((event.end_time_ms - event.start_time_ms) / 1000));
  return { summary: `视频前 ${duration} 秒检测到${displayEventType(event.event_type)}，${detectionSummary(event)}`, severity: event.severity, suggestion: '建议人工核实该规则事件及相关视频证据。', report_source: '规则化分析' };
}
export function groupEvents(events: EventItem[]) {
  const groups = new Map<string, EventItem[]>();
  for (const event of events) { const key = event.association_key ?? `${event.job_id}:${event.event_type}:${event.rule_version ?? 'rule-v1'}`; groups.set(key, [...(groups.get(key) ?? []), event]); }
  return [...groups.entries()].map(([key, items]) => ({ key, events: [...items].sort((a, b) => a.start_time_ms - b.start_time_ms) }));
}
