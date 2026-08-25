use std::env;

use sqlx::Row;
use vision_event_api::{
    application::AppState,
    domain::{Detection, JobStatus},
    persistence::{Database, DatabaseConfig},
    storage::MediaStorage,
    worker,
};
use uuid::Uuid;

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
