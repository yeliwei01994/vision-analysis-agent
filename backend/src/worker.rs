use crate::{
    application::AppState,
    domain::{Detection, Event, Evidence, JobStatus},
    rules::{EventRule, RuleEngine},
    video,
    yolo::YoloDetector,
};
use uuid::Uuid;

pub async fn process_job(state: AppState, job_id: Uuid) -> bool {
    let Some(job) = state.job(job_id) else {
        eprintln!("worker job {job_id} not found in memory or database");
        return false;
    };
    println!("worker processing job {job_id}: {}", job.filename);
    state.update_job(job_id, JobStatus::Processing, 35);
    let (frames, detector_version) = match job.source_uri.as_deref() {
        Some(source_uri) => match process_video(source_uri).await {
            Ok(result) => result,
            Err(error) => {
                eprintln!("video job {job_id} failed: {error}");
                state.update_job(job_id, JobStatus::Failed, 100);
                persist_job_state(&state, job_id, "failed").await;
                return false;
            }
        },
        None => (Vec::new(), "no-video-source".to_string()),
    };
    state.update_job(job_id, JobStatus::Processing, 75);
    let rule = state
        .rules
        .read()
        .expect("rules lock poisoned")
        .get("person_stay")
        .cloned()
        .unwrap_or_else(|| EventRule::new("person_stay", "person", 0.25, 0));
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
        event.detector_version = detector_version.clone();
        let mut evidence_frames = Vec::new();
        for frame in &candidate.frames {
            let Some(path) = frame.frame_path.as_deref() else { continue; };
            if let Some((_, _, detections)) = evidence_frames
                .iter_mut()
                .find(|(timestamp, existing_path, _): &&mut (u64, &std::path::Path, Vec<Detection>)| *timestamp == frame.timestamp_ms && *existing_path == path)
            {
                detections.push(frame.detection.clone());
            } else {
                evidence_frames.push((frame.timestamp_ms, path, vec![frame.detection.clone()]));
            }
        }
        if !evidence_frames.is_empty() {
            event.evidence = match state.storage.save_event_evidence(event.id, &evidence_frames).await {
                Ok(evidence) => evidence,
                Err(error) => {
                    eprintln!("failed to save evidence for {}: {error}", event.id);
                    Evidence::default()
                }
            };
        }
        state
            .events
            .write()
            .expect("events lock poisoned")
            .insert(event.id, event.clone());
        if let Some(database) = &state.database {
            if let Err(error) = database.save_event(&event).await {
                eprintln!("failed to save event {} for job {job_id}: {error}", event.id);
            }
        }
    }
    state.update_job(job_id, JobStatus::Completed, 100);
    if let Some(database) = &state.database {
        if let Some(job) = state.job(job_id) {
            if let Err(error) = database.save_job(&job).await {
                eprintln!("failed to save completed job {job_id}: {error}");
            }
        }
    }
    true
}

async fn persist_job_state(state: &AppState, job_id: Uuid, reason: &str) {
    let Some(database) = &state.database else {
        eprintln!("job {job_id} marked {reason}, but database is unavailable");
        return;
    };
    let Some(job) = state.job(job_id) else {
        eprintln!("job {job_id} marked {reason}, but job state is unavailable");
        return;
    };
    if let Err(error) = database.save_job(&job).await {
        eprintln!("failed to persist {reason} state for job {job_id}: {error}");
    }
}

async fn process_video(
    source_uri: &str,
) -> Result<(Vec<crate::rules::FrameDetection>, String), String> {
    let (directory, frame_paths) =
        video::extract_frames(std::path::Path::new(source_uri), 500).await?;
    let detector = YoloDetector::from_env().map_err(|error| error.to_string())?;
    let mut frames = Vec::new();
    let mut model_version = None;
    for (index, frame_path) in frame_paths.iter().enumerate() {
        let filename = frame_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "invalid frame filename".to_string())?;
        if video::frame_timestamp_ms(filename).is_none() {
            return Err(format!("invalid frame filename: {filename}"));
        }
        let timestamp_ms = index as u64 * 500;
        let response = detector.detect_frame(frame_path, timestamp_ms).await?;
        println!(
            "YOLO response: frame={}, timestamp_ms={}, detections={}, classes={:?}",
            filename,
            timestamp_ms,
            response.detections.len(),
            response
                .detections
                .iter()
                .map(|detection| detection.class_name.as_str())
                .collect::<Vec<_>>()
        );
        model_version = Some(response.model_version.clone());
        let mut detected = response.into_frame_detections();
        for detection in &mut detected {
            detection.frame_path = Some(frame_path.clone());
        }
        frames.extend(detected);
    }
    let _ = tokio::fs::remove_dir_all(directory).await;
    Ok((
        frames,
        model_version.unwrap_or_else(|| "yolo-no-detections".into()),
    ))
}

pub async fn run_loop(state: AppState, queue: crate::queue::TaskQueue) {
    println!("worker started; listening on Redis stream vision:jobs");
    loop {
        match queue.consume_once().await {
            Ok(Some(message)) => {
                println!("worker received job {} (attempt {})", message.job_id, message.attempt);
                if let Ok(job_id) = Uuid::parse_str(&message.job_id) {
                    if state.job(job_id).is_none() {
                        if let Some(database) = &state.database {
                            match database.get_job(job_id).await {
                                Ok(Some(job)) => {
                                    state
                                        .jobs
                                        .write()
                                        .expect("jobs lock poisoned")
                                        .insert(job.id, job);
                                }
                                Ok(None) => {
                                    eprintln!("worker job {job_id} was not found in MySQL");
                                }
                                Err(error) => {
                                    eprintln!("worker failed to load job {job_id} from MySQL: {error}");
                                }
                            }
                        }
                    }
                    let succeeded = process_job(state.clone(), job_id).await;
                    println!("worker finished job {job_id}: {succeeded}");
                } else {
                    eprintln!("worker received invalid job id: {}", message.job_id);
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
