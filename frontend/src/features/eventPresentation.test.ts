import { describe, expect, it } from 'vitest';
import { detectionSummary, displayEventType, fallbackAnalysis, groupEvents } from './eventPresentation';
import type { EventItem } from '../types/events';

const event: EventItem = { id: 'a', job_id: 'job', event_type: 'person_stay', rule_version: 'rule-v1', start_time_ms: 500, end_time_ms: 6000, severity: 'medium', status: 'unreviewed', confidence: .34, objects: Array.from({ length: 11 }, () => ({ class_name: 'person', confidence: .34, bbox: [10, 20, 30, 40] })), evidence: { frame_urls: [], frames: [] }, analysis: null, detector_version: 'yolo' };

describe('event presentation', () => {
  it('translates rules, summarizes detections, and groups related events', () => {
    expect(displayEventType('person_stay')).toBe('人员停留');
    expect(displayEventType('person_count_limit')).toBe('最大人员数量');
    expect(detectionSummary(event)).toBe('人员 11 次检测 · 平均置信度 34%');
    expect(fallbackAnalysis(event).summary).toContain('视频前 6 秒检测到人员停留');
    expect(groupEvents([event, { ...event, id: 'b', start_time_ms: 7000 }])).toHaveLength(1);
  });
});
