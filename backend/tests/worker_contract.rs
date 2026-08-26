use std::env;
use std::path::PathBuf;

use sqlx::Row;
use vision_event_api::{
    application::AppState,
    domain::{Detection, JobStatus},
    persistence::{Database, DatabaseConfig},
    rules::{FrameDetection, RuleEvent},
    storage::MediaStorage,
    worker,
};
use uuid::Uuid;

fn candidate(start_time_ms: u64, end_time_ms: u64, confidence: f32) -> RuleEvent {
    let detection = Detection::new("person".into(), confidence, [10.0, 20.0, 30.0, 40.0]);
    RuleEvent {
        event_type: "person_stay".into(),
        start_time_ms,
        end_time_ms,
        confidence,
        objects: vec![detection.clone()],
        frames: vec![FrameDetection { timestamp_ms: start_time_ms, detection, frame_path: None }],
        rule_version: "rule-v1".into(),
    }
}

fn frame(timestamp_ms: u64, confidence: f32) -> FrameDetection {
    FrameDetection {
        timestamp_ms,
        detection: Detection::new("person".into(), confidence, [10.0, 20.0, 30.0, 40.0]),
        frame_path: Some(PathBuf::from(format!("frame-{timestamp_ms}.jpg"))),
    }
}

#[test]
fn merges_matching_candidates_within_the_gap() {
    let events = worker::merge_rule_events(
        vec![candidate(1_000, 1_500, 0.4), candidate(0, 500, 0.8)],
        3_000,
    );

    assert_eq!(events.len(), 1);
    assert_eq!((events[0].start_time_ms, events[0].end_time_ms), (0, 1_500));
    assert_eq!(events[0].objects.len(), 2);
    assert!((events[0].confidence - 0.6).abs() < f32::EPSILON);
}

#[test]
fn keeps_candidates_separated_by_more_than_the_gap() {
    let events = worker::merge_rule_events(
        vec![candidate(0, 500, 0.8), candidate(3_501, 4_000, 0.4)],
        3_000,
    );

    assert_eq!(events.len(), 2);
}

#[test]
fn selects_first_peak_last_and_five_second_samples_without_duplicates() {
    let selected = worker::select_evidence_frames(
        &[
            frame(0, 0.4),
            frame(500, 0.99),
            frame(5_000, 0.3),
            frame(7_000, 0.8),
            frame(10_000, 0.6),
        ],
        5_000,
        12,
    );

    assert_eq!(selected.iter().map(|(timestamp, _, _)| *timestamp).collect::<Vec<_>>(), vec![0, 500, 5_000, 10_000]);
}

#[test]
fn caps_selected_evidence_at_twelve_frames() {
    let frames = (0..20)
        .map(|index| frame(index * 500, 0.5 + index as f32 / 100.0))
        .collect::<Vec<_>>();

    assert_eq!(worker::select_evidence_frames(&frames, 0, 12).len(), 12);
}

#[tokio::test]
async fn event_evidence_copies_matching_frames_under_media_root() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source-frame.jpg");
    tokio::fs::write(&source, b"jpeg-evidence").await.unwrap();
    let storage = MediaStorage::new(temporary.path().join("media"));
    let detection = Detection::new("person".into(), 0.87, [10.0, 20.0, 30.0, 40.0]);
    let event_id = Uuid::new_v4();

    let evidence = storage
        .save_event_evidence(event_id, &[(500, source.as_path(), vec![detection.clone()])])
        .await
        .unwrap();

    assert_eq!(evidence.frames.len(), 1);
    assert_eq!(evidence.frames[0].timestamp_ms, 500);
    assert_eq!(evidence.frames[0].detections[0].class_name, "person");
    assert_eq!(evidence.thumbnail_url, Some(evidence.frames[0].image_url.clone()));
    assert!(temporary
        .path()
        .join("media")
        .join("evidence")
        .join(event_id.to_string())
        .exists());
}

#[tokio::test]
async fn persisted_video_job_can_be_loaded_by_worker() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let state = AppState::default();
    let job = state.create_job("loadable-video.mp4".into(), 1_234);
    database.save_job(&job).await.unwrap();

    let loaded = database.get_job(job.id).await.unwrap().unwrap();
    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.duration_ms, 1_234);
    assert_eq!(loaded.progress, 0);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn worker_refreshes_rules_saved_by_the_api_process() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url)).await.unwrap();
    database.migrate().await.unwrap();

    let event_type = format!("test_rule_refresh_{}", Uuid::new_v4());
    let mut saved_rule = vision_event_api::rules::EventRule::new(&event_type, "person", 0.25, 0);
    saved_rule.geometry = Some(vision_event_api::rules::Geometry::polygon(vec![
        [0.2, 0.2], [0.8, 0.2], [0.8, 0.8],
    ]));
    database.save_rule(&saved_rule).await.unwrap();

    let state = AppState::default().with_integrations(Some(database.clone()), None);
    worker::refresh_rules(&state).await.unwrap();
    let loaded = state.event_rules().into_iter().find(|rule| rule.event_type == event_type).unwrap();
    assert_eq!(loaded.geometry.unwrap().points[0], [0.2, 0.2]);

    let _ = sqlx::query("DELETE FROM event_rules WHERE event_type = ?")
        .bind(event_type)
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn failed_video_processing_is_persisted_to_mysql() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let state = AppState::default().with_integrations(Some(database.clone()), None);
    let job = state.create_job("missing-video.mp4".into(), 0);
    {
        let mut job_with_source = state.job(job.id).unwrap();
        job_with_source.source_uri = Some("media/does-not-exist.mp4".into());
        state
            .jobs
            .write()
            .unwrap()
            .insert(job.id, job_with_source.clone());
        database.save_job(&job_with_source).await.unwrap();
    }

    assert!(!worker::process_job(state.clone(), job.id).await);

    let row = sqlx::query("SELECT status, progress FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(row.try_get::<String, _>("status").unwrap(), "failed");
    assert_eq!(row.try_get::<u8, _>("progress").unwrap(), 100);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
    assert_eq!(state.job(job.id).unwrap().status, JobStatus::Failed);
}
