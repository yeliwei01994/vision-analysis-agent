use crate::{
    adapters::{Detector, MockDetector},
    application::AppState,
    domain::{Event, JobStatus},
    rules::{EventRule, RuleEngine},
};
use uuid::Uuid;

pub async fn process_job(state: AppState, job_id: Uuid) -> bool {
    if state.job(job_id).is_none() {
        return false;
    }
    state.update_job(job_id, JobStatus::Processing, 35);
    tokio::task::yield_now().await;
    state.update_job(job_id, JobStatus::Processing, 75);
    let detector = MockDetector;
    let frames = [1_000_u64, 1_600, 2_600]
        .into_iter()
        .flat_map(|timestamp| detector.detect(timestamp))
        .collect::<Vec<_>>();
    let rule = state
        .rules
        .read()
        .expect("rules lock poisoned")
        .get("person_stay")
        .cloned()
        .unwrap_or_else(|| EventRule::new("person_stay", "person", 0.8, 1_000));
    for candidate in RuleEngine::new(rule).evaluate(&frames) {
        let mut event = Event::new(
            job_id,
            candidate.event_type,
            candidate.start_time_ms,
            candidate.end_time_ms,
            candidate.objects,
        );
        event.confidence = candidate.confidence;
        event.rule_version = candidate.rule_version;
        event.detector_version = detector.version().into();
        state
            .events
            .write()
            .expect("events lock poisoned")
            .insert(event.id, event.clone());
        if let Some(database) = &state.database {
            let _ = database.save_event(&event).await;
        }
    }
    state.update_job(job_id, JobStatus::Completed, 100);
    if let Some(database) = &state.database {
        if let Some(job) = state.job(job_id) {
            let _ = database.save_job(&job).await;
        }
    }
    true
}

pub async fn run_loop(state: AppState, queue: crate::queue::TaskQueue) {
    loop {
        match queue.consume_once().await {
            Ok(Some(message)) => {
                if let Ok(job_id) = Uuid::parse_str(&message.job_id) {
                    if state.job(job_id).is_none() {
                        if let Some(database) = &state.database {
                            if let Ok(Some(job)) = database.get_job(job_id).await {
                                state
                                    .jobs
                                    .write()
                                    .expect("jobs lock poisoned")
                                    .insert(job.id, job);
                            }
                        }
                    }
                    let _ = process_job(state.clone(), job_id).await;
                }
            }
            Ok(None) => tokio::task::yield_now().await,
            Err(error) => {
                eprintln!("redis queue error: {error}");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}
