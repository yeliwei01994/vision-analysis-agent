use crate::{
    application::AppState,
    domain::{Detection, Event, Evidence, JobStatus},
    rules::RuleEngine,
    video,
    yolo::YoloDetector,
};
use std::path::PathBuf;
use uuid::Uuid;

pub async fn process_job(state: AppState, job_id: Uuid) -> bool {
    let Some(job) = state.job(job_id) else {
        eprintln!("worker job {job_id} not found in memory or database");
        return false;
    };
    println!("worker processing job {job_id}: {}", job.filename);
    state.update_job(job_id, JobStatus::Processing, 35);
    let (frame_directory, frames, detector_version) = match job.source_uri.as_deref() {
        Some(source_uri) => match process_video(source_uri).await {
            Ok(result) => result,
            Err(error) => {
                eprintln!("video job {job_id} failed: {error}");
                state.update_job(job_id, JobStatus::Failed, 100);
                persist_job_state(&state, job_id, "failed").await;
                return false;
            }
        },
        None => (None, Vec::new(), "no-video-source".to_string()),
    };
    state.update_job(job_id, JobStatus::Processing, 75);
    let rules = state.event_rules();
    for rule in rules.into_iter().filter(|rule| rule.enabled) {
    for candidate in merge_rule_events(RuleEngine::new(rule).evaluate(&frames), 3_000) {
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
        let selected_frames = select_evidence_frames(&candidate.frames, 5_000, 12);
        let evidence_frames = selected_frames
            .iter()
            .map(|(timestamp_ms, path, detections)| (*timestamp_ms, path.as_path(), detections.clone()))
            .collect::<Vec<_>>();
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
    }
    if let Some(directory) = frame_directory {
        let _ = tokio::fs::remove_dir_all(directory).await;
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

pub fn merge_rule_events(mut candidates: Vec<crate::rules::RuleEvent>, gap_ms: u64) -> Vec<crate::rules::RuleEvent> {
    candidates.sort_by_key(|candidate| candidate.start_time_ms);
    let mut merged: Vec<crate::rules::RuleEvent> = Vec::new();
    for candidate in candidates {
        let active = merged.iter_mut().rev().find(|active| {
            active.event_type == candidate.event_type
                && active.rule_version == candidate.rule_version
                && primary_class(active) == primary_class(&candidate)
                && candidate.start_time_ms.saturating_sub(active.end_time_ms) <= gap_ms
        });
        if let Some(active) = active {
            combine_rule_events(active, candidate);
        } else {
            merged.push(candidate);
        }
    }
    merged.sort_by_key(|candidate| candidate.start_time_ms);
    merged
}

fn primary_class(event: &crate::rules::RuleEvent) -> Option<&str> {
    event.objects.first().map(|detection| detection.class_name.as_str())
}

fn combine_rule_events(active: &mut crate::rules::RuleEvent, candidate: crate::rules::RuleEvent) {
    let active_count = active.objects.len();
    let candidate_count = candidate.objects.len();
    let total_count = active_count + candidate_count;
    if total_count > 0 {
        active.confidence = (active.confidence * active_count as f32
            + candidate.confidence * candidate_count as f32)
            / total_count as f32;
    }
    active.start_time_ms = active.start_time_ms.min(candidate.start_time_ms);
    active.end_time_ms = active.end_time_ms.max(candidate.end_time_ms);
    active.objects.extend(candidate.objects);
    active.frames.extend(candidate.frames);
    active.frames.sort_by_key(|frame| frame.timestamp_ms);
}

pub fn select_evidence_frames(
    frames: &[crate::rules::FrameDetection],
    sample_interval_ms: u64,
    max_frames: usize,
) -> Vec<(u64, PathBuf, Vec<Detection>)> {
    if max_frames == 0 {
        return Vec::new();
    }
    let mut sources: Vec<(u64, PathBuf, Vec<Detection>)> = Vec::new();
    for frame in frames {
        let Some(path) = frame.frame_path.as_ref() else { continue; };
        if let Some((_, _, detections)) = sources.iter_mut().find(|(timestamp, existing_path, _)| {
            *timestamp == frame.timestamp_ms && *existing_path == *path
        }) {
            detections.push(frame.detection.clone());
        } else {
            sources.push((frame.timestamp_ms, path.clone(), vec![frame.detection.clone()]));
        }
    }
    sources.sort_by_key(|(timestamp_ms, _, _)| *timestamp_ms);
    if sources.is_empty() {
        return sources;
    }

    let peak_index = sources
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| {
            let left_confidence = left.2.iter().map(|detection| detection.confidence).fold(0.0, f32::max);
            let right_confidence = right.2.iter().map(|detection| detection.confidence).fold(0.0, f32::max);
            left_confidence.total_cmp(&right_confidence)
        })
        .map(|(index, _)| index)
        .unwrap_or(0);

    let mut selected = vec![0, peak_index, sources.len() - 1];
    let mut last_sampled_at = sources[0].0;
    for (index, (timestamp_ms, _, _)) in sources.iter().enumerate().skip(1) {
        if timestamp_ms.saturating_sub(last_sampled_at) >= sample_interval_ms {
            selected.push(index);
            last_sampled_at = *timestamp_ms;
        }
    }
    selected.sort_unstable();
    selected.dedup();

    let mandatory = [0, peak_index, sources.len() - 1];
    let mut kept = mandatory.into_iter().collect::<Vec<_>>();
    kept.sort_unstable();
    kept.dedup();
    for index in selected {
        if kept.len() >= max_frames { break; }
        if !kept.contains(&index) { kept.push(index); }
    }
    kept.sort_unstable();
    kept.into_iter().take(max_frames).map(|index| sources[index].clone()).collect()
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
) -> Result<(Option<std::path::PathBuf>, Vec<crate::rules::FrameDetection>, String), String> {
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
    Ok((
        Some(directory),
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
