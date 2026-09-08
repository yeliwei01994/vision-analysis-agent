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
    progress::RedisProgressStore,
};

#[tokio::test]
async fn publish_job_progress_emits_snake_case_stage_and_monotonic_sequence() {
    let state = AppState::default();
    let job = state.create_job("progress.mp4".into(), 12_000);
    let mut subscriber = state.subscribe_job_progress();

    state.publish_job_progress(job.id, JobStage::Preparing, 5, "queued".into()).await.unwrap();
    state.publish_job_progress(job.id, JobStage::ExtractingFrames, 25, "frames ready".into()).await.unwrap();

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
            "sequence": 1,
            "attempt": 0
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

        let published = state.publish_job_progress(job.id, JobStage::Detecting, 25, "stale update".into()).await.unwrap();
        let saved = state.job(job.id).unwrap();

        assert_eq!(saved.status, status);
        assert_eq!(saved.progress, 100);
        assert!(published.is_none(), "a stale processing update must not publish a fake terminal event");
        assert!(tokio::time::timeout(Duration::from_millis(20), subscriber.recv()).await.is_err());
    }
}

#[test]
fn snapshot_reconciliation_keeps_terminal_state_until_a_new_attempt_starts() {
    let state = AppState::default();
    let mut terminal = state.create_job("terminal-snapshot.mp4".into(), 12_000);
    terminal.status = JobStatus::Failed;
    terminal.progress = 35;
    terminal.stage = Some(JobStage::ExtractingFrames);
    terminal.status_message = Some("视频帧提取失败".into());
    state.reconcile_job(terminal.clone());

    let mut stale = terminal.clone();
    stale.status = JobStatus::Processing;
    stale.progress = 1;
    stale.stage = Some(JobStage::Preparing);
    stale.status_message = Some("正在准备视频".into());
    state.reconcile_job(stale);

    let preserved = state.job(terminal.id).unwrap();
    assert_eq!(preserved.status, JobStatus::Failed);
    assert_eq!(preserved.stage, Some(JobStage::ExtractingFrames));
    assert_eq!(preserved.status_message.as_deref(), Some("视频帧提取失败"));

    let mut retry = terminal;
    retry.attempt += 1;
    retry.status = JobStatus::Pending;
    retry.progress = 0;
    retry.stage = Some(JobStage::Preparing);
    retry.status_message = Some("等待重新处理".into());
    state.reconcile_job(retry.clone());

    assert_eq!(state.job(retry.id).unwrap().status, JobStatus::Pending);
    assert_eq!(state.job(retry.id).unwrap().attempt, retry.attempt);
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

    state.publish_job_progress(job.id, JobStage::Preparing, 0, "queued".into()).await.unwrap();

    let frame = tokio::time::timeout(Duration::from_secs(1), async move {
        let mut body = response.into_body();
        loop {
            let frame = body.frame().await;
            if frame
                .as_ref()
                .and_then(|frame| frame.as_ref().ok())
                .and_then(|frame| frame.data_ref())
                .is_some_and(|data| {
                    std::str::from_utf8(data)
                        .is_ok_and(|payload| payload.contains("event: job-progress"))
                })
            {
                break frame;
            }
        }
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

#[tokio::test]
async fn progress_stream_sends_a_full_snapshot_on_connect_and_after_local_lag() {
    let state = AppState::default();
    let job = state.create_job("snapshot.mp4".into(), 12_000);
    let response = api::router(state.clone())
        .oneshot(
            Request::get("/api/v1/jobs/progress/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();

    let initial = tokio::time::timeout(Duration::from_secs(1), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let initial = std::str::from_utf8(&initial).unwrap();
    assert!(initial.contains("event: job-snapshot"));
    assert!(initial.contains(&job.id.to_string()));
    assert!(initial.contains("\"reason\":\"connected\""));

    for progress in 1..=180 {
        state
            .publish_job_progress(
                job.id,
                JobStage::Detecting,
                progress.min(99) as u8,
                format!("frame {progress}"),
            )
            .await
            .unwrap();
    }

    let lag_snapshot = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let frame = body.frame().await.unwrap().unwrap();
            let data = frame.into_data().unwrap();
            let payload = std::str::from_utf8(&data).unwrap().to_string();
            if payload.contains("event: job-snapshot")
                && payload.contains("\"reason\":\"lagged\"")
            {
                break payload;
            }
        }
    })
    .await
    .expect("broadcast lag must force a full snapshot");
    assert!(lag_snapshot.contains("\"progress\":99"));
}

#[tokio::test]
async fn redis_progress_from_a_separate_worker_state_reaches_the_api_sse_route() {
    dotenvy::dotenv().ok();
    let Ok(redis_url) = std::env::var("REDIS_URL") else {
        eprintln!("REDIS_URL is required for the cross-process progress integration test");
        return;
    };
    let stream = format!("vision:test:progress:{}", uuid::Uuid::new_v4());
    let progress_store = RedisProgressStore::new(&redis_url, &stream).unwrap();
    progress_store.clear().await.unwrap();

    let worker_state = AppState::default().with_progress_store(Some(progress_store.clone()));
    let api_state = AppState::default().with_progress_store(Some(progress_store.clone()));
    let job = worker_state.create_job("cross-process.mp4".into(), 12_000);
    api_state.reconcile_job(job.clone());

    let response = api::router(api_state)
        .oneshot(
            Request::get("/api/v1/jobs/progress/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let initial = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(std::str::from_utf8(&initial)
        .unwrap()
        .contains("event: job-snapshot"));

    worker_state
        .publish_job_progress(job.id, JobStage::Detecting, 35, "worker detected objects".into())
        .await
        .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let frame = body.frame().await.unwrap().unwrap();
            let data = frame.into_data().expect("expected SSE data frame");
            let text = std::str::from_utf8(&data).unwrap().to_string();
            if text.contains("event: job-progress") {
                break text;
            }
        }
    })
    .await
    .expect("worker progress should reach the API SSE response through Redis");

    assert!(payload.contains("worker detected objects"));
    assert!(payload.contains("\"progress\":35"));
    progress_store.clear().await.unwrap();
}
