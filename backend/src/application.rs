use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::adapters::{MockAnalyzer, VisionAnalyzer};
use crate::domain::{Detection, Event, EventStatus, JobStatus, VideoJob};
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
}

impl Default for AppState {
    fn default() -> Self {
        let mut rules = HashMap::new();
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
        }
    }
}

impl AppState {
    pub fn with_integrations(
        mut self,
        database: Option<Database>,
        queue: Option<TaskQueue>,
    ) -> Self {
        self.database = database;
        self.queue = queue;
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
        if let Some(job) = self.jobs.write().expect("jobs lock poisoned").get_mut(&id) {
            job.status = status;
            job.progress = progress;
        }
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
