use vision_event_api::application::AppState;
use vision_event_api::domain::Detection;
use vision_event_api::rules::{point_in_polygon, EventRule, FrameDetection, Geometry, RuleEngine};

fn person(timestamp_ms: u64, confidence: f32, track_id: u64) -> FrameDetection {
    FrameDetection {
        timestamp_ms,
        detection: Detection {
            class_name: "person".into(),
            confidence,
            bbox: [0.0, 0.0, 10.0, 20.0],
            track_id: Some(track_id),
        },
        frame_path: None,
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

#[test]
fn worker_default_rule_accepts_detected_person_for_local_testing() {
    let rule = AppState::default()
        .event_rules()
        .into_iter()
        .find(|rule| rule.event_type == "person_stay")
        .expect("default person_stay rule");
    assert_eq!(rule.min_confidence, 0.25);
    assert_eq!(rule.min_duration_ms, 0);
}

fn zone_rule(event_type: &str) -> EventRule {
    let mut rule = EventRule::new(event_type, "person", 0.25, 1_000);
    rule.geometry = Some(Geometry::polygon(vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]]));
    rule
}

fn zoned_person(timestamp_ms: u64, confidence: f32) -> FrameDetection {
    FrameDetection {
        timestamp_ms,
        detection: Detection {
            class_name: "person".into(),
            confidence,
            bbox: [0.2, 0.2, 0.2, 0.2],
            track_id: Some(1),
        },
        frame_path: None,
    }
}

#[test]
fn polygon_treats_boundary_points_as_inside() {
    let polygon = vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
    assert!(point_in_polygon([0.1, 0.5], &polygon));
    assert!(point_in_polygon([0.5, 0.5], &polygon));
    assert!(!point_in_polygon([0.95, 0.5], &polygon));
}

#[test]
fn intrusion_requires_detection_inside_the_zone() {
    let events = RuleEngine::new(zone_rule("person_enter_zone"))
        .evaluate(&[zoned_person(0, 0.9)]);
    assert_eq!(events.len(), 1);
}

#[test]
fn disabled_rule_never_emits_events() {
    let mut rule = zone_rule("person_enter_zone");
    rule.enabled = false;
    assert!(RuleEngine::new(rule).evaluate(&[zoned_person(0, 0.9)]).is_empty());
}

#[test]
fn count_limit_requires_more_than_the_configured_threshold() {
    let mut rule = zone_rule("person_count_limit");
    rule.threshold = Some(1);
    let events = RuleEngine::new(rule).evaluate(&[zoned_person(0, 0.9), zoned_person(0, 0.8)]);
    assert_eq!(events.len(), 1);
}
