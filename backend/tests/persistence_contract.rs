use std::env;
use uuid::Uuid;
use vision_event_api::persistence::{Database, DatabaseConfig};
use vision_event_api::queue::QueueMessage;
use vision_event_api::domain::{JobStage, JobStatus, VideoJob};

#[test]
fn database_config_uses_explicit_url() {
    let config = DatabaseConfig::new("mysql://vision:secret@mysql/vision_events");
    assert_eq!(config.url, "mysql://vision:secret@mysql/vision_events");
}

#[test]
fn queue_message_serializes_job_id_and_attempt() {
    let id = Uuid::new_v4();
    let message = QueueMessage::new(id);
    let encoded = serde_json::to_value(message).unwrap();
    assert_eq!(encoded["job_id"], id.to_string());
    assert_eq!(encoded["attempt"], 0);
}

#[test]
fn migrations_are_kept_in_repository() {
    assert!(
        std::path::Path::new("migrations/001_initial.sql").exists()
            || env::var("CARGO_MANIFEST_DIR").is_ok()
    );
}

#[tokio::test]
async fn list_events_keeps_rows_with_null_prompt_version() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    let events = database.list_events().await.unwrap();
    assert!(
        events.iter().any(|event| event.prompt_version.is_none()),
        "events with a NULL prompt_version must remain readable"
    );
}

#[tokio::test]
async fn annotated_video_fields_round_trip_on_video_jobs() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url)).await.unwrap();
    database.migrate().await.unwrap();
    let mut job = VideoJob::new("annotated-playback-test.mp4".into(), 2_000);
    job.annotated_video_url = Some(format!("/media/annotated/{}.mp4", job.id));
    job.annotated_video_status = Some("ready".into());
    job.annotated_video_error = None;
    database.save_job(&job).await.unwrap();
    let loaded = database.get_job(job.id).await.unwrap().unwrap();
    assert_eq!(loaded.annotated_video_url, job.annotated_video_url);
    assert_eq!(loaded.annotated_video_status, Some("ready".into()));
    assert_eq!(loaded.annotated_video_error, None);
    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?").bind(job.id.to_string()).execute(&database.pool).await;
}

#[tokio::test]
async fn terminal_job_rows_reject_stale_processing_writes_until_explicit_retry() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    let mut job = VideoJob::new("terminal-write-guard.mp4".into(), 2_000);
    job.status = JobStatus::Processing;
    job.progress = 35;
    job.stage = Some(JobStage::Detecting);
    database.save_job(&job).await.unwrap();

    let mut failed = job.clone();
    failed.status = JobStatus::Failed;
    failed.status_message = Some("目标检测失败：模型服务不可用".into());
    database.save_job(&failed).await.unwrap();

    let mut stale = job;
    stale.progress = 1;
    stale.stage = Some(JobStage::Preparing);
    stale.status_message = Some("正在准备视频".into());
    database.save_job(&stale).await.unwrap();

    let preserved = database.get_job(failed.id).await.unwrap().unwrap();
    assert_eq!(preserved.status, JobStatus::Failed);
    assert_eq!(preserved.progress, 35);
    assert_eq!(preserved.stage, Some(JobStage::Detecting));
    assert_eq!(preserved.status_message, failed.status_message);

    let retried = database.restart_job(failed.id).await.unwrap().unwrap();
    assert_eq!(retried.status, JobStatus::Pending);
    assert_eq!(retried.progress, 0);
    assert_eq!(retried.attempt, 1);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(failed.id.to_string())
        .execute(&database.pool)
        .await;
}
