use crate::domain::{AnalysisResult, Detection, Event};
use crate::rules::FrameDetection;

pub trait VisionAnalyzer {
    fn analyze(&self, event: &Event) -> AnalysisResult;
}

pub struct MockAnalyzer;

pub trait Detector: Send + Sync {
    fn detect(&self, frame_timestamp_ms: u64) -> Vec<FrameDetection>;
    fn version(&self) -> &str;
}

pub struct MockDetector;

impl Detector for MockDetector {
    fn detect(&self, frame_timestamp_ms: u64) -> Vec<FrameDetection> {
        vec![FrameDetection {
            timestamp_ms: frame_timestamp_ms,
            detection: Detection {
                class_name: "person".into(),
                confidence: 0.94,
                bbox: [10.0, 20.0, 80.0, 160.0],
                track_id: Some(1),
            },
        }]
    }
    fn version(&self) -> &str {
        "mock-detector-v1"
    }
}

impl VisionAnalyzer for MockAnalyzer {
    fn analyze(&self, event: &Event) -> AnalysisResult {
        AnalysisResult {
            summary: format!("检测到事件：{}", event.event_type),
            severity: event.severity.clone(),
            suggestion: "请进行人工复核".into(),
            report_source: "mock".into(),
        }
    }
}
