use crate::domain::Detection;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Geometry {
    pub kind: String,
    pub points: Vec<[f32; 2]>,
}

impl Geometry {
    pub fn polygon(points: Vec<[f32; 2]>) -> Self { Self { kind: "polygon".into(), points } }
}

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
    #[serde(default)]
    pub geometry: Option<Geometry>,
    #[serde(default)]
    pub threshold: Option<u32>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool { true }

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
            geometry: None,
            threshold: None,
            enabled: true,
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
        if !self.rule.enabled { return Vec::new(); }
        let needs_zone = matches!(self.rule.event_type.as_str(), "person_enter_zone" | "person_count_limit");
        if needs_zone && self.rule.geometry.is_none() { return Vec::new(); }
        let within_zone = |frame: &&FrameDetection| self.rule.geometry.as_ref().map_or(true, |geometry| {
            point_in_polygon(bottom_center(&frame.detection), &geometry.points)
        });
        let mut tracks: HashMap<u64, Vec<&FrameDetection>> = HashMap::new();
        for frame in frames.iter().filter(|frame| {
            frame.detection.class_name == self.rule.class_name
                && frame.detection.confidence >= self.rule.min_confidence
        }).filter(within_zone) {
            tracks
                .entry(frame.detection.track_id.unwrap_or(0))
                .or_default()
                .push(frame);
        }
        if self.rule.event_type == "person_count_limit" {
            let threshold = self.rule.threshold.unwrap_or(0) as usize;
            let mut by_timestamp: HashMap<u64, Vec<&FrameDetection>> = HashMap::new();
            for frame in frames.iter().filter(|frame| frame.detection.class_name == self.rule.class_name && frame.detection.confidence >= self.rule.min_confidence).filter(within_zone) {
                by_timestamp.entry(frame.timestamp_ms).or_default().push(frame);
            }
            return by_timestamp.into_iter().filter_map(|(timestamp_ms, matches)| {
                (matches.len() > threshold).then(|| RuleEvent { event_type: self.rule.event_type.clone(), start_time_ms: timestamp_ms, end_time_ms: timestamp_ms, confidence: matches.iter().map(|frame| frame.detection.confidence).sum::<f32>() / matches.len() as f32, objects: matches.iter().map(|frame| frame.detection.clone()).collect(), frames: matches.iter().map(|frame| (*frame).clone()).collect(), rule_version: self.rule.version.clone() })
            }).collect();
        }
        tracks
            .into_values()
            .filter_map(|track| {
                let start = track.first()?.timestamp_ms;
                let end = track.last()?.timestamp_ms;
                if self.rule.event_type != "person_enter_zone" && end.saturating_sub(start) < self.rule.min_duration_ms {
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

fn bottom_center(detection: &Detection) -> [f32; 2] {
    [detection.bbox[0] + detection.bbox[2] / 2.0, detection.bbox[1] + detection.bbox[3]]
}

pub fn point_in_polygon(point: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    if polygon.len() < 3 { return false; }
    let mut inside = false;
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let cross = (point[1] - current[1]) * (next[0] - current[0]) - (point[0] - current[0]) * (next[1] - current[1]);
        if cross.abs() < f32::EPSILON && point[0] >= current[0].min(next[0]) && point[0] <= current[0].max(next[0]) && point[1] >= current[1].min(next[1]) && point[1] <= current[1].max(next[1]) { return true; }
        if (current[1] > point[1]) != (next[1] > point[1]) && point[0] < (next[0] - current[0]) * (point[1] - current[1]) / (next[1] - current[1]) + current[0] { inside = !inside; }
    }
    inside
}
