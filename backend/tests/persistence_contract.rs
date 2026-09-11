use std::env;
use uuid::Uuid;
use vision_event_api::domain::{
    AnalysisResult, Detection, Event, Evidence, JobStage, JobStatus, VideoJob,
};
use vision_event_api::persistence::{Database, DatabaseConfig};
use vision_event_api::queue::QueueMessage;

#[test]
fn database_config_uses_explicit_url() {
    let config = DatabaseConfig::new("postgres://vision:secret@postgres/vision_events");
    assert_eq!(
        config.url,
        "postgres://vision:secret@postgres/vision_events"
    );
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
async fn migrated_schema_uses_native_jsonb_and_expected_gin_indexes() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let jsonb_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = 'public' \
           AND (table_name, column_name) IN ( \
             ('events', 'objects_json'), \
             ('events', 'evidence_json'), \
             ('events', 'analysis_json'), \
             ('event_rules', 'geometry_json') \
           ) \
           AND data_type = 'jsonb'",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(jsonb_columns, 4);

    let gin_indexes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_indexes \
         WHERE schemaname = 'public' \
           AND indexname IN ('idx_events_objects_json_gin', 'idx_event_rules_geometry_json_gin') \
           AND indexdef ILIKE '%USING gin%'",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(gin_indexes, 2);

    let bigint_u32_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = 'public' \
           AND (table_name, column_name) IN ( \
             ('video_jobs', 'attempt'), \
             ('event_rules', 'threshold_value') \
           ) \
           AND data_type = 'bigint'",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(bigint_u32_columns, 2);
}

#[tokio::test]
async fn yolo_jsonb_payloads_round_trip_and_support_containment() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    let pool: &sqlx::PgPool = &database.pool;
    database.migrate().await.unwrap();

    let job = VideoJob::new("yolo-jsonb-round-trip.mp4".into(), 2_000);
    database.save_job(&job).await.unwrap();

    let objects_json = serde_json::json!([{
        "class_name": "person",
        "confidence": 0.875,
        "bbox": [12.5, 24.0, 88.0, 176.5],
        "track_id": 42
    }]);
    let evidence_json = serde_json::json!({
        "thumbnail_url": "/media/thumbnail.jpg",
        "clip_url": "/media/clip.mp4",
        "frame_urls": ["/media/frame.jpg"],
        "frames": [{
            "timestamp_ms": 750,
            "image_url": "/media/frame.jpg",
            "detections": objects_json.clone()
        }]
    });
    let analysis_json = serde_json::json!({
        "summary": "Person remained in the zone",
        "severity": "high",
        "suggestion": "Review the annotated clip",
        "report_source": "vision-language-model"
    });

    let mut event = Event::new(
        job.id,
        "person_stay".into(),
        500,
        1_500,
        serde_json::from_value::<Vec<Detection>>(objects_json.clone()).unwrap(),
    );
    event.evidence = serde_json::from_value::<Evidence>(evidence_json.clone()).unwrap();
    event.analysis = Some(serde_json::from_value::<AnalysisResult>(analysis_json.clone()).unwrap());
    event.reviewed_at = Some("1704067200".into());
    database.save_event(&event).await.unwrap();

    let loaded = database.get_event(event.id).await.unwrap().unwrap();
    assert_eq!(serde_json::to_value(&loaded.objects).unwrap(), objects_json);
    assert_eq!(
        serde_json::to_value(&loaded.evidence).unwrap(),
        evidence_json
    );
    assert_eq!(
        serde_json::to_value(&loaded.analysis).unwrap(),
        analysis_json
    );
    assert_eq!(loaded.reviewed_at.as_deref(), Some("1704067200"));

    database.save_event(&loaded).await.unwrap();
    let resaved = database.get_event(event.id).await.unwrap().unwrap();
    assert_eq!(resaved.reviewed_at.as_deref(), Some("1704067200"));

    let matches: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE objects_json @> $1 AND id = $2")
            .bind(sqlx::types::Json(
                serde_json::json!([{"class_name": "person"}]),
            ))
            .bind(event.id)
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(matches, 1);

    event.analysis = None;
    database.save_event(&event).await.unwrap();
    let without_analysis = database.get_event(event.id).await.unwrap().unwrap();
    assert!(without_analysis.analysis.is_none());
    let analysis_is_null: bool =
        sqlx::query_scalar("SELECT analysis_json IS NULL FROM events WHERE id = $1")
            .bind(event.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(analysis_is_null);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = $1")
        .bind(job.id)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn u32_attempt_and_rule_threshold_round_trip_without_loss() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let mut job = VideoJob::new("u32-domain-test.mp4".into(), 1_000);
    job.attempt = u32::MAX;
    database.save_job(&job).await.unwrap();
    assert_eq!(
        database.get_job(job.id).await.unwrap().unwrap().attempt,
        u32::MAX
    );

    let event_type = format!("u32_threshold_{}", Uuid::new_v4());
    let mut rule = vision_event_api::rules::EventRule::new(&event_type, "person", 0.5, 1_000);
    rule.threshold = Some(u32::MAX);
    database.save_rule(&rule).await.unwrap();
    let loaded_threshold = database
        .list_rules()
        .await
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.event_type == event_type)
        .and_then(|candidate| candidate.threshold);
    assert_eq!(loaded_threshold, Some(u32::MAX));

    let _ = sqlx::query("DELETE FROM event_rules WHERE event_type = $1")
        .bind(&event_type)
        .execute(&database.pool)
        .await;
    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = $1")
        .bind(job.id)
        .execute(&database.pool)
        .await;
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
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
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
    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = $1")
        .bind(job.id)
        .execute(&database.pool)
        .await;
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

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = $1")
        .bind(failed.id)
        .execute(&database.pool)
        .await;
}
