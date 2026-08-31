use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use std::time::Duration;
use tower::ServiceExt;
use vision_event_api::{
    api,
    application::AppState,
    domain::{JobStage, JobStatus},
};

#[tokio::test]
async fn publish_job_progress_emits_snake_case_stage_and_monotonic_sequence() {
    let state = AppState::default();
    let job = state.create_job("progress.mp4".into(), 12_000);
    let mut subscriber = state.subscribe_job_progress();

    state.publish_job_progress(job.id, JobStage::Preparing, 5, "queued".into());
    state.publish_job_progress(job.id, JobStage::ExtractingFrames, 25, "frames ready".into());

    let first = subscriber.recv().await.unwrap();
    let second = subscriber.recv().await.unwrap();

    assert_eq!(first.job_id, job.id);
    assert_eq!(first.status, JobStatus::Processing);
    assert_eq!(first.sequence, 1);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        json!({
            "job_id": job.id,
            "status": "processing",
            "stage": "preparing",
            "progress": 5,
            "message": "queued",
            "updated_at": first.updated_at,
            "estimated_remaining_ms": null,
            "sequence": 1
        })
    );

    assert_eq!(second.sequence, 2);
    assert_eq!(
        serde_json::to_value(&second).unwrap()["stage"],
        "extracting_frames"
    );
}

#[test]
fn terminal_jobs_do_not_regress_to_processing_updates() {
    let state = AppState::default();
    let job = state.create_job("terminal.mp4".into(), 12_000);

    state.update_job(job.id, JobStatus::Completed, 100);
    state.update_job(job.id, JobStatus::Processing, 42);

    let saved = state.job(job.id).unwrap();
    assert_eq!(saved.status, JobStatus::Completed);
    assert_eq!(saved.progress, 100);
}

#[tokio::test]
async fn publish_job_progress_preserves_terminal_state_and_emits_stored_snapshot() {
    let state = AppState::default();
    let mut subscriber = state.subscribe_job_progress();

    for status in [JobStatus::Completed, JobStatus::Failed, JobStatus::Cancelled] {
        let job = state.create_job(format!("{status:?}.mp4").to_lowercase(), 12_000);
        state.update_job(job.id, status.clone(), 100);

        state.publish_job_progress(job.id, JobStage::Finalizing, 25, "stale update".into());

        let event = subscriber.recv().await.unwrap();
        let saved = state.job(job.id).unwrap();

        assert_eq!(saved.status, status);
        assert_eq!(saved.progress, 100);
        assert_eq!(event.job_id, job.id);
        assert_eq!(event.status, saved.status);
        assert_eq!(event.progress, saved.progress);
        assert_eq!(event.stage, JobStage::Finalizing);
        assert_eq!(event.message, "stale update");
    }
}

#[tokio::test]
async fn progress_stream_returns_sse_content_type_and_json_events() {
    let state = AppState::default();
    let job = state.create_job("stream.mp4".into(), 12_000);
    let response = api::router(state.clone())
        .oneshot(
            Request::get("/api/v1/jobs/progress/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "text/event-stream");

    state.publish_job_progress(job.id, JobStage::Preparing, 0, "queued".into());

    let frame = tokio::time::timeout(Duration::from_secs(1), async move {
        response.into_body().frame().await
    })
    .await
    .expect("expected SSE frame before timeout")
    .expect("expected SSE frame")
    .expect("expected SSE body frame");
    let body = frame.into_data().expect("expected SSE data frame");
    let payload = std::str::from_utf8(&body).unwrap();

    assert!(payload.contains("event: job-progress"));
    assert!(payload.contains("\"stage\":\"preparing\""));
}
