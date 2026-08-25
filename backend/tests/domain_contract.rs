use serde_json::json;
use vision_event_api::domain::{Detection, Event, EventStatus, Evidence, JobStatus, VideoJob};

#[test]
fn serializes_video_job_and_event_contract() {
    let job = VideoJob::new("warehouse.mp4".into(), 12_000);
    let event = Event::new(
        job.id,
        "person_enter_zone".into(),
        1_000,
        12_000,
        vec![Detection::new(
            "person".into(),
            0.94,
            [10.0, 20.0, 80.0, 160.0],
        )],
    );

    let encoded = serde_json::to_value(json!({ "job": job, "event": event })).unwrap();
    assert_eq!(encoded["job"]["status"], "pending");
    assert_eq!(encoded["event"]["status"], "unreviewed");
    assert_eq!(encoded["event"]["event_type"], "person_enter_zone");
    assert_eq!(
        encoded["event"]["evidence"]["thumbnail_url"],
        serde_json::Value::Null
    );
}

#[test]
fn status_values_are_stable_api_strings() {
    assert_eq!(
        serde_json::to_string(&JobStatus::Processing).unwrap(),
        "\"processing\""
    );
    assert_eq!(
        serde_json::to_string(&EventStatus::Confirmed).unwrap(),
        "\"confirmed\""
    );
    assert_eq!(
        serde_json::to_string(&Evidence::default()).unwrap(),
        "{\"thumbnail_url\":null,\"clip_url\":null,\"frame_urls\":[],\"frames\":[]}"
    );
}
