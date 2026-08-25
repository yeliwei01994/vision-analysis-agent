use crate::domain::Detection;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FrameDetection {
    pub timestamp_ms: u64,
    pub detection: Detection,
    pub frame_path: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventRule {
    pub event_type: String,
    pub class_name: String,
    pub min_confidence: f32,
    pub min_duration_ms: u64,
    pub version: String,
}

impl EventRule {
    pub fn new(
        event_type: impl Into<String>,
        class_name: impl Into<String>,
        min_confidence: f32,
        min_duration_ms: u64,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            class_name: class_name.into(),
            min_confidence,
            min_duration_ms,
            version: "rule-v1".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleEvent {
    pub event_type: String,
    pub start_time_ms: u64,
    pub end_time_ms: u64,
    pub confidence: f32,
    pub objects: Vec<Detection>,
    pub frames: Vec<FrameDetection>,
    pub rule_version: String,
}

pub struct RuleEngine {
    rule: EventRule,
}

impl RuleEngine {
    pub fn new(rule: EventRule) -> Self {
        Self { rule }
    }

    pub fn evaluate(&self, frames: &[FrameDetection]) -> Vec<RuleEvent> {
        let mut tracks: HashMap<u64, Vec<&FrameDetection>> = HashMap::new();
        for frame in frames.iter().filter(|frame| {
            frame.detection.class_name == self.rule.class_name
                && frame.detection.confidence >= self.rule.min_confidence
        }) {
            tracks
                .entry(frame.detection.track_id.unwrap_or(0))
                .or_default()
                .push(frame);
        }
        tracks
            .into_values()
            .filter_map(|track| {
                let start = track.first()?.timestamp_ms;
                let end = track.last()?.timestamp_ms;
                if end.saturating_sub(start) < self.rule.min_duration_ms {
                    return None;
                }
                let confidence = track
                    .iter()
                    .map(|frame| frame.detection.confidence)
                    .sum::<f32>()
                    / track.len() as f32;
                Some(RuleEvent {
                    event_type: self.rule.event_type.clone(),
                    start_time_ms: start,
                    end_time_ms: end,
                    confidence,
                    objects: track.iter().map(|frame| frame.detection.clone()).collect(),
                    frames: track.iter().map(|frame| (*frame).clone()).collect(),
                    rule_version: self.rule.version.clone(),
                })
            })
            .collect()
    }
}
