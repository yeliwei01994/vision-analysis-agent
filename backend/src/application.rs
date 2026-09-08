use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::adapters::{MockAnalyzer, VisionAnalyzer};
use crate::domain::{
    unix_time_millis, Detection, Event, EventStatus, JobProgressEvent, JobStage, JobStatus,
    VideoJob,
};
use crate::progress::RedisProgressStore;
use crate::rules::EventRule;
use crate::storage::MediaStorage;
use crate::{persistence::Database, queue::TaskQueue};

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<Uuid, VideoJob>>>,
    pub events: Arc<RwLock<HashMap<Uuid, Event>>>,
    pub storage: MediaStorage,
    pub rules: Arc<RwLock<HashMap<String, EventRule>>>,
    pub database: Option<Database>,
    pub queue: Option<TaskQueue>,
    pub job_progress_events: broadcast::Sender<JobProgressEvent>,
    pub job_progress_sequences: Arc<RwLock<HashMap<Uuid, u64>>>,
    pub job_progress_snapshots: Arc<RwLock<HashMap<Uuid, JobProgressEvent>>>,
    pub progress_store: Option<RedisProgressStore>,
}

impl Default for AppState {
    fn default() -> Self {
        let mut rules = HashMap::new();
        let (job_progress_events, _) = broadcast::channel(128);
        rules.insert(
            "person_stay".into(),
            EventRule::new("person_stay", "person", 0.25, 0),
        );
        Self {
            jobs: Arc::default(),
            events: Arc::default(),
            storage: MediaStorage::default(),
            rules: Arc::new(RwLock::new(rules)),
            database: None,
            queue: None,
            job_progress_events,
            job_progress_sequences: Arc::default(),
            job_progress_snapshots: Arc::default(),
            progress_store: None,
        }
    }
}

impl AppState {
    fn apply_job_update(
        &self,
        id: Uuid,
        status: JobStatus,
        progress: u8,
        stage: Option<JobStage>,
        message: Option<String>,
    ) -> Option<(VideoJob, bool)> {
        let mut jobs = self.jobs.write().expect("jobs lock poisoned");
        let job = jobs.get_mut(&id)?;
        if job.status.is_terminal() && !status.is_terminal() {
            return Some((job.clone(), false));
        }
        job.status = status;
        job.progress = progress;
        if stage.is_some() {
            job.stage = stage;
        }
        if message.is_some() {
            job.status_message = message;
        }
        job.updated_at = Some(unix_time_millis());
        Some((job.clone(), true))
    }

    pub fn with_integrations(
        mut self,
        database: Option<Database>,
        queue: Option<TaskQueue>,
    ) -> Self {
        self.database = database;
        self.queue = queue;
        self
    }

    pub fn with_progress_store(mut self, progress_store: Option<RedisProgressStore>) -> Self {
        self.progress_store = progress_store;
        self
    }

    pub fn create_job(&self, filename: String, duration_ms: u64) -> VideoJob {
        let job = VideoJob::new(filename, duration_ms);
        self.jobs
            .write()
            .expect("jobs lock poisoned")
            .insert(job.id, job.clone());
        job
    }

    pub fn seed_event(&self, job: &VideoJob) -> Event {
        let detection = Detection {
            class_name: "person".into(),
            confidence: 0.94,
            bbox: [10.0, 20.0, 80.0, 160.0],
            track_id: Some(1),
        };
        let mut event = Event::new(
            job.id,
            "person_enter_zone".into(),
            1_000,
            job.duration_ms.min(12_000),
            vec![detection],
        );
        event.confidence = 0.91;
        event.analysis = Some(MockAnalyzer.analyze(&event));
        self.events
            .write()
            .expect("events lock poisoned")
            .insert(event.id, event.clone());
        event
    }

    pub fn job(&self, id: Uuid) -> Option<VideoJob> {
        self.jobs
            .read()
            .expect("jobs lock poisoned")
            .get(&id)
            .cloned()
    }
    pub fn jobs(&self) -> Vec<VideoJob> {
        self.jobs
            .read()
            .expect("jobs lock poisoned")
            .values()
            .cloned()
            .collect()
    }
    pub fn update_job_filename(&self, id: Uuid, filename: String) -> Option<VideoJob> {
        let mut jobs = self.jobs.write().expect("jobs lock poisoned");
        let job = jobs.get_mut(&id)?;
        job.filename = filename;
        job.updated_at = Some(unix_time_millis());
        Some(job.clone())
    }
    pub fn delete_job(&self, id: Uuid) -> Result<(), JobStatus> {
        let mut jobs = self.jobs.write().expect("jobs lock poisoned");
        let job = jobs.get(&id).ok_or(JobStatus::Failed)?;
        if matches!(job.status, JobStatus::Processing) {
            return Err(job.status.clone());
        }
        jobs.remove(&id);
        drop(jobs);
        self.events.write().expect("events lock poisoned").retain(|_, event| event.job_id != id);
        Ok(())
    }
    pub fn forget_job(&self, id: Uuid) {
        self.jobs.write().expect("jobs lock poisoned").remove(&id);
        self.events.write().expect("events lock poisoned").retain(|_, event| event.job_id != id);
    }
    pub fn event(&self, id: Uuid) -> Option<Event> {
        self.events
            .read()
            .expect("events lock poisoned")
            .get(&id)
            .cloned()
    }
    pub fn review_event(&self, id: Uuid, status: EventStatus) -> Option<Event> {
        let mut events = self.events.write().expect("events lock poisoned");
        let event = events.get_mut(&id)?;
        event.status = status;
        Some(event.clone())
    }
    pub fn events(&self) -> Vec<Event> {
        self.events
            .read()
            .expect("events lock poisoned")
            .values()
            .cloned()
            .collect()
    }
    pub fn complete_job(&self, id: Uuid) {
        self.update_job(id, JobStatus::Completed, 100);
    }
    pub fn update_job(&self, id: Uuid, status: JobStatus, progress: u8) {
        let _ = self.apply_job_update(id, status, progress, None, None);
    }

    pub fn update_job_progress(
        &self,
        id: Uuid,
        status: JobStatus,
        progress: u8,
        stage: Option<JobStage>,
        message: Option<String>,
    ) -> Option<VideoJob> {
        self.apply_job_update(id, status, progress, stage, message)
            .and_then(|(job, applied)| applied.then_some(job))
    }

    pub fn restart_job(&self, id: Uuid) -> Option<VideoJob> {
        let mut jobs = self.jobs.write().expect("jobs lock poisoned");
        let job = jobs.get_mut(&id)?;
        if !matches!(job.status, JobStatus::Failed | JobStatus::Cancelled) {
            return Some(job.clone());
        }
        job.attempt = job.attempt.saturating_add(1);
        job.status = JobStatus::Pending;
        job.progress = 0;
        job.stage = Some(JobStage::Preparing);
        job.status_message = Some("等待重新处理".into());
        job.annotated_video_url = None;
        job.annotated_video_status = None;
        job.annotated_video_error = None;
        job.updated_at = Some(unix_time_millis());
        Some(job.clone())
    }

    pub fn reconcile_job(&self, incoming: VideoJob) -> VideoJob {
        let mut jobs = self.jobs.write().expect("jobs lock poisoned");
        if let Some(current) = jobs.get(&incoming.id) {
            let older_attempt = incoming.attempt < current.attempt;
            let same_attempt_terminal_regression =
                incoming.attempt == current.attempt && current.status.is_terminal();
            let stale_timestamp = incoming.attempt == current.attempt
                && current
                    .updated_at
                    .zip(incoming.updated_at)
                    .is_some_and(|(current_at, incoming_at)| incoming_at < current_at);
            if older_attempt || same_attempt_terminal_regression || stale_timestamp {
                return current.clone();
            }
        }
        jobs.insert(incoming.id, incoming.clone());
        incoming
    }

    pub async fn publish_job_progress(
        &self,
        job_id: Uuid,
        stage: JobStage,
        progress: u8,
        message: String,
    ) -> redis::RedisResult<Option<JobProgressEvent>> {
        let Some(job) = self.update_job_progress(
            job_id,
            JobStatus::Processing,
            progress,
            Some(stage),
            Some(message),
        ) else {
            return Ok(None);
        };
        self.publish_job_snapshot(job).await.map(Some)
    }

    pub async fn publish_current_job_progress(
        &self,
        job_id: Uuid,
    ) -> redis::RedisResult<Option<JobProgressEvent>> {
        let Some(job) = self.job(job_id) else {
            return Ok(None);
        };
        self.publish_job_snapshot(job).await.map(Some)
    }

    async fn publish_job_snapshot(
        &self,
        job: VideoJob,
    ) -> redis::RedisResult<JobProgressEvent> {
        let event = if let Some(progress_store) = &self.progress_store {
            progress_store
                .publish(JobProgressEvent::from_job(&job, 0))
                .await?
                .event
        } else {
            let sequence = {
            let mut sequences = self
                .job_progress_sequences
                .write()
                .expect("job progress sequences lock poisoned");
                let next = sequences.get(&job.id).copied().unwrap_or(0) + 1;
                sequences.insert(job.id, next);
            next
        };
            JobProgressEvent::from_job(&job, sequence)
        };
        self.job_progress_sequences
            .write()
            .expect("job progress sequences lock poisoned")
            .insert(job.id, event.sequence);
        self.job_progress_snapshots
            .write()
            .expect("job progress snapshots lock poisoned")
            .insert(job.id, event.clone());
        let _ = self.job_progress_events.send(event);
        Ok(self
            .job_progress_snapshots
            .read()
            .expect("job progress snapshots lock poisoned")
            .get(&job.id)
            .cloned()
            .expect("published job progress snapshot should exist"))
    }

    pub fn subscribe_job_progress(&self) -> broadcast::Receiver<JobProgressEvent> {
        self.job_progress_events.subscribe()
    }

    pub fn job_progress_snapshot(&self) -> Vec<JobProgressEvent> {
        let stored = self
            .job_progress_snapshots
            .read()
            .expect("job progress snapshots lock poisoned");
        let sequences = self
            .job_progress_sequences
            .read()
            .expect("job progress sequences lock poisoned");
        let mut snapshot = self
            .jobs()
            .into_iter()
            .map(|job| {
                stored.get(&job.id).cloned().unwrap_or_else(|| {
                    JobProgressEvent::from_job(
                        &job,
                        sequences.get(&job.id).copied().unwrap_or(0),
                    )
                })
            })
            .collect::<Vec<_>>();
        snapshot.sort_by_key(|event| (event.updated_at, event.job_id));
        snapshot
    }
    pub fn event_rules(&self) -> Vec<EventRule> {
        self.rules
            .read()
            .expect("rules lock poisoned")
            .values()
            .cloned()
            .collect()
    }
    pub fn update_rule(&self, event_type: String, rule: EventRule) -> EventRule {
        self.rules
            .write()
            .expect("rules lock poisoned")
            .insert(event_type, rule.clone());
        rule
    }
}
