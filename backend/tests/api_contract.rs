use axum::{
    body::Body,
    http::{header::{CACHE_CONTROL, CONTENT_TYPE}, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use std::env;
use tower::ServiceExt;
use vision_event_api::{
    api,
    application::AppState,
    persistence::{Database, DatabaseConfig},
    storage::MediaStorage,
};

fn app() -> Router {
    api::router(AppState::default())
}

#[tokio::test]
async fn media_route_serves_evidence_images_and_rejects_unsafe_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("media");
    let image = root.join("evidence").join("event-1").join("frame-0001.jpg");
    tokio::fs::create_dir_all(image.parent().unwrap()).await.unwrap();
    tokio::fs::write(&image, b"jpeg-evidence").await.unwrap();
    let mut state = AppState::default();
    state.storage = MediaStorage::new(root);
    let router = api::router(state);

    let image_response = router
        .clone()
        .oneshot(Request::get("/media/evidence/event-1/frame-0001.jpg").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(image_response.status(), StatusCode::OK);
    assert_eq!(image_response.headers()[CONTENT_TYPE], "image/jpeg");
    assert_eq!(image_response.headers()[CACHE_CONTROL], "private, max-age=3600");

    let unsafe_response = router
        .oneshot(Request::get("/media/%2E%2E/Cargo.toml").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(!unsafe_response.status().is_success());
}

#[tokio::test]
async fn deleting_an_event_removes_its_evidence_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("media");
    let mut state = AppState::default();
    state.storage = MediaStorage::new(root.clone());
    let job = state.create_job("delete-evidence.mp4".into(), 1_000);
    let event = state.seed_event(&job);
    let directory = root.join("evidence").join(event.id.to_string());
    tokio::fs::create_dir_all(&directory).await.unwrap();
    tokio::fs::write(directory.join("frame.jpg"), b"jpeg-evidence").await.unwrap();

    let response = api::router(state)
        .oneshot(
            Request::delete(format!("/api/v1/events/{}", event.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(!directory.exists());
}

#[tokio::test]
async fn health_returns_ok() {
    let response = app()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_job_loads_completed_job_from_mysql_when_api_memory_is_empty() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    let state = AppState::default();
    let mut job = state.create_job("completed-from-worker.mp4".into(), 1_000);
    job.status = vision_event_api::domain::JobStatus::Completed;
    job.progress = 100;
    database.save_job(&job).await.unwrap();

    let response = api::router(AppState::default().with_integrations(Some(database.clone()), None))
        .oneshot(
            Request::get(format!("/api/v1/jobs/{}", job.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "completed");
    assert_eq!(body["progress"], 100);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn get_job_prefers_mysql_over_stale_api_memory_state() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    let state = AppState::default().with_integrations(Some(database.clone()), None);
    let mut job = state.create_job("stale-memory.mp4".into(), 1_000);
    database.save_job(&job).await.unwrap();
    job.status = vision_event_api::domain::JobStatus::Processing;
    job.progress = 1;
    state.jobs.write().unwrap().insert(job.id, job.clone());

    let mut completed = job.clone();
    completed.status = vision_event_api::domain::JobStatus::Completed;
    completed.progress = 100;
    database.save_job(&completed).await.unwrap();

    let response = api::router(state)
        .oneshot(
            Request::get(format!("/api/v1/jobs/{}", job.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "completed");
    assert_eq!(body["progress"], 100);

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn create_video_and_list_seed_event() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"demo.mp4","duration_ms":12000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = service
        .oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let events: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(events.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn upload_then_process_video_reports_processing_failure_for_invalid_media() {
    let service = app();
    let body = b"--demo\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\nfake-video\r\n--demo--\r\n";
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos/upload")
                .header("content-type", "multipart/form-data; boundary=demo")
                .body(Body::from(body.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = payload["id"].as_str().unwrap();
    assert!(payload["source_uri"]
        .as_str()
        .unwrap()
        .ends_with("clip.mp4"));

    let response = service
        .oneshot(
            Request::post(format!("/api/v1/videos/{id}/process"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let processed: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(processed["status"], "failed");
    assert_eq!(processed["progress"], 100);
}

#[tokio::test]
async fn upload_accepts_video_larger_than_axum_default_body_limit() {
    let service = app();
    let payload = vec![b'x'; 3 * 1024 * 1024];
    let mut body = b"--large\r\nContent-Disposition: form-data; name=\"file\"; filename=\"large.mp4\"\r\nContent-Type: video/mp4\r\n\r\n".to_vec();
    body.extend_from_slice(&payload);
    body.extend_from_slice(b"\r\n--large--\r\n");
    let response = service
        .oneshot(
            Request::post("/api/v1/videos/upload")
                .header("content-type", "multipart/form-data; boundary=large")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn event_rules_can_be_listed_and_updated() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::get("/api/v1/event-rules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let defaults: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(defaults
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["event_type"] == "person_stay"));

    let response = service
        .oneshot(
            Request::put("/api/v1/event-rules/person_stay")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"class_name":"person","min_confidence":0.75,"min_duration_ms":2000,"geometry":{"kind":"polygon","points":[[0.1,0.1],[0.9,0.1],[0.9,0.9]]},"threshold":2,"enabled":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(updated["min_duration_ms"], 2000);
    assert_eq!(updated["geometry"]["kind"], "polygon");
    assert_eq!(updated["threshold"], 2);
    assert_eq!(updated["enabled"], false);
}

#[tokio::test]
async fn event_can_be_confirmed_and_ignored() {
    let service = app();
    let create = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"filename":"review.mp4","duration_ms":12000}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    let events_response = service
        .clone()
        .oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let events: serde_json::Value = serde_json::from_slice(
        &events_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let id = events[0]["id"].as_str().unwrap();

    let confirmed = service
        .clone()
        .oneshot(
            Request::post(format!("/api/v1/events/{id}/confirm"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    let confirmed_body: serde_json::Value =
        serde_json::from_slice(&confirmed.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(confirmed_body["status"], "confirmed");

    let ignored = service
        .oneshot(
            Request::post(format!("/api/v1/events/{id}/ignore"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ignored.status(), StatusCode::OK);
    let ignored_body: serde_json::Value =
        serde_json::from_slice(&ignored.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(ignored_body["status"], "ignored");
}

#[tokio::test]
async fn event_query_export_and_report_endpoints_return_operational_outputs() {
    let service = app();
    let create = service.clone().oneshot(Request::post("/api/v1/videos").header("content-type", "application/json").body(Body::from(r#"{"filename":"operations.mp4","duration_ms":1000}"#)).unwrap()).await.unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let events_response = service.clone().oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap()).await.unwrap();
    let events: serde_json::Value = serde_json::from_slice(&events_response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = events[0]["id"].as_str().unwrap();
    let query = service.clone().oneshot(Request::get("/api/v1/events/query?status=unreviewed&page=1&page_size=1").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(query.status(), StatusCode::OK);
    let page: serde_json::Value = serde_json::from_slice(&query.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(page["page_size"], 1);
    assert!(page["total"].as_u64().unwrap() >= 1);
    let csv = service.clone().oneshot(Request::get("/api/v1/events/export.csv?status=unreviewed").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(csv.status(), StatusCode::OK);
    assert!(String::from_utf8(csv.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap().contains("event_type"));
    let report = service.oneshot(Request::get(format!("/api/v1/events/{id}/report.html")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(report.status(), StatusCode::OK);
}

#[tokio::test]
async fn persisted_worker_event_can_be_confirmed_from_a_fresh_api_process() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let worker_state = AppState::default();
    let job = worker_state.create_job("worker-event.mp4".into(), 1_000);
    let event = worker_state.seed_event(&job);
    database.save_job(&job).await.unwrap();
    database.save_event(&event).await.unwrap();

    let response = api::router(AppState::default().with_integrations(Some(database.clone()), None))
        .oneshot(
            Request::post(format!("/api/v1/events/{}/confirm", event.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "confirmed");

    let saved = database
        .list_events()
        .await
        .unwrap()
        .into_iter()
        .find(|saved| saved.id == event.id)
        .unwrap();
    assert_eq!(saved.status, vision_event_api::domain::EventStatus::Confirmed);

    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(event.id.to_string())
        .execute(&database.pool)
        .await;
    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn persisted_worker_event_can_be_deleted_from_a_fresh_api_process() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    database.migrate().await.unwrap();

    let worker_state = AppState::default();
    let job = worker_state.create_job("delete-worker-event.mp4".into(), 1_000);
    let event = worker_state.seed_event(&job);
    database.save_job(&job).await.unwrap();
    database.save_event(&event).await.unwrap();

    let response = api::router(AppState::default().with_integrations(Some(database.clone()), None))
        .oneshot(
            Request::delete(format!("/api/v1/events/{}", event.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(database.get_event(event.id).await.unwrap().is_none());

    let _ = sqlx::query("DELETE FROM video_jobs WHERE id = ?")
        .bind(job.id.to_string())
        .execute(&database.pool)
        .await;
}

#[tokio::test]
async fn jobs_can_be_listed() {
    let service = app();
    for filename in ["one.mp4", "two.mp4"] {
        let response = service
            .clone()
            .oneshot(
                Request::post("/api/v1/videos")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"filename":"{filename}","duration_ms":1000}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = service
        .oneshot(Request::get("/api/v1/jobs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn completed_job_can_be_renamed() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"before.mp4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let id = created["id"].as_str().unwrap();

    let response = service
        .oneshot(
            Request::put(format!("/api/v1/jobs/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"after.mp4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let renamed: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(renamed["filename"], "after.mp4");
}

#[tokio::test]
async fn job_delete_removes_job_and_events_but_rejects_processing_job() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"delete-me.mp4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let id = created["id"].as_str().unwrap();
    let response = service
        .clone()
        .oneshot(
            Request::delete(format!("/api/v1/jobs/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = service
        .oneshot(Request::get("/api/v1/jobs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let jobs: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert!(jobs.as_array().unwrap().iter().all(|job| job["id"] != id));
}

#[tokio::test]
async fn job_mutations_validate_names_and_processing_deletes() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"processing.mp4"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    let id = created["id"].as_str().unwrap();
    let too_long = "x".repeat(256);
    let response = service
        .clone()
        .oneshot(
            Request::put(format!("/api/v1/jobs/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"filename": too_long}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let state = AppState::default();
    let job = state.create_job("processing.mp4".into(), 0);
    state.update_job(job.id, vision_event_api::domain::JobStatus::Processing, 1);
    let response = api::router(state)
        .oneshot(
            Request::delete(format!("/api/v1/jobs/{}", job.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn pending_job_can_be_deleted() {
    let state = AppState::default();
    let job = state.create_job("pending.mp4".into(), 0);
    let response = api::router(state)
        .oneshot(
            Request::delete(format!("/api/v1/jobs/{}", job.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
