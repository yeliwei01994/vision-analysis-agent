use vision_event_api::domain::Detection;
use vision_event_api::rules::{EventRule, FrameDetection, RuleEngine};

fn person(timestamp_ms: u64, confidence: f32, track_id: u64) -> FrameDetection {
    FrameDetection {
        timestamp_ms,
        detection: Detection {
            class_name: "person".into(),
            confidence,
            bbox: [0.0, 0.0, 10.0, 20.0],
            track_id: Some(track_id),
        },
    }
}

#[test]
fn ignores_low_confidence_detections() {
    let rule = EventRule::new("person_enter_zone", "person", 0.8, 1_000);
    let events = RuleEngine::new(rule).evaluate(&[person(0, 0.5, 1), person(2_000, 0.6, 1)]);
    assert!(events.is_empty());
}

#[test]
fn creates_event_after_track_exceeds_minimum_duration() {
    let rule = EventRule::new("person_stay", "person", 0.8, 1_000);
    let events = RuleEngine::new(rule).evaluate(&[
        person(0, 0.95, 1),
        person(400, 0.92, 1),
        person(1_100, 0.9, 1),
    ]);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "person_stay");
    assert_eq!(events[0].start_time_ms, 0);
    assert_eq!(events[0].end_time_ms, 1_100);
}
