export type EventStatus = 'unreviewed' | 'confirmed' | 'ignored' | 'processing' | 'resolved' | 'closed';

export interface Detection { class_name: string; confidence: number; bbox: [number, number, number, number]; track_id?: number; }
export interface EvidenceFrame { timestamp_ms: number; image_url: string; detections: Detection[]; }
export interface Evidence { thumbnail_url?: string | null; clip_url?: string | null; frame_urls: string[]; frames?: EvidenceFrame[]; }
export interface AnalysisResult { summary: string; severity: string; suggestion: string; report_source: string; }
export interface EventItem { id: string; job_id: string; event_type: string; start_time_ms: number; end_time_ms: number; severity: string; status: EventStatus; confidence: number; objects: Detection[]; evidence: Evidence; analysis?: AnalysisResult | null; rule_version?: string; detector_version: string; reviewer?: string | null; reviewed_at?: string | null; review_note?: string | null; disposition?: string | null; zone_key?: string | null; association_key?: string | null; related_event_ids?: string[]; }
export interface EventReview { id: string; event_id: string; old_status: EventStatus; new_status: EventStatus; reviewer?: string | null; note?: string | null; disposition?: string | null; created_at: string; }
export interface EventPage { items: EventItem[]; total: number; page: number; page_size: number; }
export interface VideoJob { id: string; filename: string; duration_ms: number; status: string; progress: number; source_uri?: string | null; annotated_video_url?: string | null; annotated_video_status?: 'pending' | 'ready' | 'failed' | null; annotated_video_error?: string | null; }
export interface RuleGeometry { kind: 'polygon'; points: [number, number][]; }
export interface EventRule { event_type: string; class_name: string; min_confidence: number; min_duration_ms: number; version: string; geometry?: RuleGeometry | null; threshold?: number | null; enabled?: boolean; }
