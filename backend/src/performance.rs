use serde::Serialize;
use std::{collections::BTreeMap, time::Instant};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PerformanceSummary {
    pub job_id: Uuid,
    pub total_ms: u128,
    pub stages_ms: BTreeMap<String, u128>,
    pub frames: usize,
    pub batches: usize,
    pub detections: usize,
    pub events: usize,
    pub batch_size: usize,
    pub concurrency: usize,
    pub errors: usize,
}

impl PerformanceSummary {
    pub fn new(job_id: Uuid) -> Self {
        Self {
            job_id,
            total_ms: 0,
            stages_ms: BTreeMap::new(),
            frames: 0,
            batches: 0,
            detections: 0,
            events: 0,
            batch_size: 0,
            concurrency: 0,
            errors: 0,
        }
    }

    pub fn add_stage_ms(&mut self, stage: impl Into<String>, elapsed_ms: u128) {
        *self.stages_ms.entry(stage.into()).or_default() += elapsed_ms;
    }

    pub fn stage_ms(&self, stage: &str) -> Option<u128> {
        self.stages_ms.get(stage).copied()
    }

    pub fn stage_sample_count(&self) -> usize {
        0
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("performance summary should serialize")
    }
}

pub struct StageTimer<'a> {
    summary: &'a mut PerformanceSummary,
    stage: &'static str,
    started_at: Instant,
}

impl<'a> StageTimer<'a> {
    pub fn start(summary: &'a mut PerformanceSummary, stage: &'static str) -> Self {
        Self { summary, stage, started_at: Instant::now() }
    }
}

impl Drop for StageTimer<'_> {
    fn drop(&mut self) {
        self.summary.add_stage_ms(self.stage, self.started_at.elapsed().as_millis());
    }
}

#[cfg(test)]
mod tests {
    use super::PerformanceSummary;
    use uuid::Uuid;

    #[test]
    fn serializes_stable_summary_fields() {
        let mut summary = PerformanceSummary::new(Uuid::nil());
        summary.add_stage_ms("frame_extract", 120);
        summary.frames = 10;
        summary.batch_size = 4;
        let value: serde_json::Value = serde_json::from_str(&summary.to_json_line()).unwrap();
        assert_eq!(value["job_id"], Uuid::nil().to_string());
        assert_eq!(value["stages_ms"]["frame_extract"], 120);
        assert_eq!(value["frames"], 10);
        assert_eq!(value["batch_size"], 4);
    }

    #[test]
    fn accumulates_stage_duration_without_per_sample_storage() {
        let mut summary = PerformanceSummary::new(Uuid::nil());
        summary.add_stage_ms("yolo_inference", 10);
        summary.add_stage_ms("yolo_inference", 25);
        assert_eq!(summary.stage_ms("yolo_inference"), Some(35));
        assert_eq!(summary.stage_sample_count(), 0);
    }
}
