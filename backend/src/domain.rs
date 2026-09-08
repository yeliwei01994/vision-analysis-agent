use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    Preparing,
    Reading,
    ExtractingFrames,
    Detecting,
    AnalyzingEvents,
    GeneratingPlayback,
    Finalizing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobProgressEvent {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub stage: Option<JobStage>,
    pub progress: u8,
    pub message: Option<String>,
    pub updated_at: u64,
    pub estimated_remaining_ms: Option<u64>,
    pub sequence: u64,
    #[serde(default)]
    pub attempt: u32,
}

impl JobProgressEvent {
    pub fn new(
        job_id: Uuid,
        status: JobStatus,
        stage: Option<JobStage>,
        progress: u8,
        message: Option<String>,
        sequence: u64,
        attempt: u32,
    ) -> Self {
        Self {
            job_id,
            status,
            stage,
            progress,
            message,
            updated_at: unix_time_millis(),
            estimated_remaining_ms: None,
            sequence,
            attempt,
        }
    }

    pub fn from_job(job: &VideoJob, sequence: u64) -> Self {
        Self {
            job_id: job.id,
            status: job.status.clone(),
            stage: job.stage.clone(),
            progress: job.progress,
            message: job.status_message.clone(),
            updated_at: job.updated_at.unwrap_or_else(unix_time_millis),
            estimated_remaining_ms: None,
            sequence,
            attempt: job.attempt,
        }
    }
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
    #[serde(default)]
    pub annotated_video_url: Option<String>,
    #[serde(default)]
    pub annotated_video_status: Option<String>,
    #[serde(default)]
    pub annotated_video_error: Option<String>,
    #[serde(default)]
    pub stage: Option<JobStage>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<u64>,
}

impl VideoJob {
    pub fn new(filename: String, duration_ms: u64) -> Self {
        let now = unix_time_millis();
        Self {
            id: Uuid::new_v4(),
            filename,
            duration_ms,
            status: JobStatus::Pending,
            progress: 0,
            source_uri: None,
            annotated_video_url: None,
            annotated_video_status: None,
            annotated_video_error: None,
            stage: None,
            status_message: None,
            attempt: 0,
            created_at: Some(now),
            updated_at: Some(now),
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
