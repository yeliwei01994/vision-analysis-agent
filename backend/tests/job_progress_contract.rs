use serde_json::json;
use vision_event_api::{
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
