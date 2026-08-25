use crate::domain::{AnalysisResult, Event};

pub trait VisionAnalyzer {
    fn analyze(&self, event: &Event) -> AnalysisResult;
}

pub struct MockAnalyzer;

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
