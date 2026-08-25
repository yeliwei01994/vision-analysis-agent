use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Unreviewed,
    Confirmed,
    Ignored,
    Processing,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJob {
    pub id: Uuid,
    pub filename: String,
    pub duration_ms: u64,
    pub status: JobStatus,
    pub progress: u8,
    pub source_uri: Option<String>,
}

impl VideoJob {
    pub fn new(filename: String, duration_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename,
            duration_ms,
            status: JobStatus::Pending,
            progress: 0,
            source_uri: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub class_name: String,
    pub confidence: f32,
    pub bbox: [f32; 4],
    pub track_id: Option<u64>,
}

impl Detection {
    pub fn new(class_name: String, confidence: f32, bbox: [f32; 4]) -> Self {
        Self {
            class_name,
            confidence,
            bbox,
            track_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceFrame {
    pub timestamp_ms: u64,
    pub image_url: String,
    #[serde(default)]
    pub detections: Vec<Detection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Evidence {
    pub thumbnail_url: Option<String>,
    pub clip_url: Option<String>,
    #[serde(default)]
    pub frame_urls: Vec<String>,
    #[serde(default)]
    pub frames: Vec<EvidenceFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub job_id: Uuid,
    pub event_type: String,
    pub start_time_ms: u64,
    pub end_time_ms: u64,
    pub severity: String,
    pub status: EventStatus,
    pub confidence: f32,
    pub objects: Vec<Detection>,
    pub evidence: Evidence,
    pub analysis: Option<AnalysisResult>,
    pub rule_version: String,
    pub prompt_version: Option<String>,
    pub detector_version: String,
    #[serde(default)]
    pub reviewer: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    #[serde(default)]
    pub review_note: Option<String>,
    #[serde(default)]
    pub disposition: Option<String>,
    #[serde(default)]
    pub zone_key: Option<String>,
    #[serde(default)]
    pub association_key: Option<String>,
    #[serde(default)]
    pub related_event_ids: Vec<Uuid>,
}

impl Event {
    pub fn new(
        job_id: Uuid,
        event_type: String,
        start_time_ms: u64,
        end_time_ms: u64,
        objects: Vec<Detection>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            job_id,
            event_type,
            start_time_ms,
            end_time_ms,
            severity: "medium".into(),
            status: EventStatus::Unreviewed,
            confidence: 0.0,
            objects,
            evidence: Evidence::default(),
            analysis: None,
            rule_version: "rule-v1".into(),
            prompt_version: None,
            detector_version: "yolo-pending".into(),
            reviewer: None,
            reviewed_at: None,
            review_note: None,
            disposition: None,
            zone_key: None,
            association_key: None,
            related_event_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReview {
    pub id: Uuid,
    pub event_id: Uuid,
    pub old_status: EventStatus,
    pub new_status: EventStatus,
    pub reviewer: Option<String>,
    pub note: Option<String>,
    pub disposition: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub summary: String,
    pub severity: String,
    pub suggestion: String,
    pub report_source: String,
}
