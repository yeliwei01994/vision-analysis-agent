use crate::{
    domain::{Event, EventReview, EventStatus, JobStage, JobStatus, VideoJob},
    rules::EventRule,
};
use sqlx::types::Json;
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgPool, Row,
};
use uuid::Uuid;

#[derive(Debug)]
pub enum JobMutationError {
    NotFound,
    Conflict,
    Database(sqlx::Error),
}

#[derive(Clone)]
pub struct Database {
    pub pool: PgPool,
}

#[derive(Clone, Debug)]
pub struct DatabaseConfig {
    pub url: String,
}

impl DatabaseConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("../db/migrations").run(&self.pool).await
    }

    pub async fn save_job(&self, job: &VideoJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO video_jobs (id, filename, duration_ms, status, progress, source_uri, annotated_video_url, annotated_video_status, annotated_video_error, progress_stage, status_message, attempt) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (id) DO UPDATE SET \
             progress = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.progress ELSE video_jobs.progress END, \
             source_uri = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.source_uri ELSE video_jobs.source_uri END, \
             annotated_video_url = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.annotated_video_url ELSE video_jobs.annotated_video_url END, \
             annotated_video_status = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.annotated_video_status ELSE video_jobs.annotated_video_status END, \
             annotated_video_error = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.annotated_video_error ELSE video_jobs.annotated_video_error END, \
             progress_stage = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.progress_stage ELSE video_jobs.progress_stage END, \
             status_message = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.status_message ELSE video_jobs.status_message END, \
             status = CASE WHEN EXCLUDED.attempt > video_jobs.attempt OR (EXCLUDED.attempt = video_jobs.attempt AND video_jobs.status NOT IN ('completed','failed','cancelled')) THEN EXCLUDED.status ELSE video_jobs.status END, \
             attempt = GREATEST(video_jobs.attempt, EXCLUDED.attempt)",
        )
            .bind(job.id)
            .bind(&job.filename)
            .bind(job.duration_ms as i64)
            .bind(status_name(&job.status))
            .bind(i16::from(job.progress))
            .bind(&job.source_uri)
            .bind(&job.annotated_video_url)
            .bind(&job.annotated_video_status)
            .bind(&job.annotated_video_error)
            .bind(job.stage.as_ref().map(stage_name))
            .bind(&job.status_message)
            .bind(i64::from(job.attempt))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Option<VideoJob>, sqlx::Error> {
        let row = sqlx::query("SELECT id, filename, duration_ms, status, progress, source_uri, annotated_video_url, annotated_video_status, annotated_video_error, progress_stage, status_message, attempt, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_ms, (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms FROM video_jobs WHERE id = $1 AND deleted_at IS NULL")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().and_then(job_from_row))
    }

    pub async fn restart_job(&self, id: Uuid) -> Result<Option<VideoJob>, sqlx::Error> {
        sqlx::query(
            "UPDATE video_jobs SET status = 'pending', progress = 0, progress_stage = 'preparing', status_message = '等待重新处理', attempt = attempt + 1, annotated_video_url = NULL, annotated_video_status = NULL, annotated_video_error = NULL \
             WHERE id = $1 AND deleted_at IS NULL AND status IN ('failed', 'cancelled')",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        self.get_job(id).await
    }

    pub async fn update_job_filename(
        &self,
        id: Uuid,
        filename: &str,
    ) -> Result<Option<VideoJob>, sqlx::Error> {
        let result =
            sqlx::query("UPDATE video_jobs SET filename = $1 WHERE id = $2 AND deleted_at IS NULL")
                .bind(filename)
                .bind(id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_job(id).await
    }

    pub async fn soft_delete_job(&self, id: Uuid) -> Result<(), JobMutationError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(JobMutationError::Database)?;
        let row = sqlx::query(
            "SELECT status FROM video_jobs WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(JobMutationError::Database)?;
        let Some(row) = row else {
            return Err(JobMutationError::NotFound);
        };
        let status: String = row.try_get("status").map_err(JobMutationError::Database)?;
        if status == "processing" {
            return Err(JobMutationError::Conflict);
        }
        sqlx::query("DELETE FROM events WHERE job_id = $1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(JobMutationError::Database)?;
        sqlx::query("UPDATE video_jobs SET deleted_at = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(JobMutationError::Database)?;
        transaction
            .commit()
            .await
            .map_err(JobMutationError::Database)
    }

    pub async fn list_jobs(&self) -> Result<Vec<VideoJob>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, filename, duration_ms, status, progress, source_uri, annotated_video_url, annotated_video_status, annotated_video_error, progress_stage, status_message, attempt, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_ms, (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms FROM video_jobs WHERE deleted_at IS NULL ORDER BY updated_at DESC, created_at DESC LIMIT 200")
            .fetch_all(&self.pool).await?;
        Ok(rows.iter().filter_map(job_from_row).collect())
    }

    pub async fn save_event(&self, event: &Event) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO events (id, job_id, event_type, start_time_ms, end_time_ms, severity, status, confidence, objects_json, evidence_json, analysis_json, rule_version, prompt_version, detector_version, reviewer, reviewed_at, review_note, disposition, zone_key, association_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, to_timestamp($16::double precision), $17, $18, $19, $20) \
             ON CONFLICT (id) DO UPDATE SET \
             status = EXCLUDED.status, analysis_json = EXCLUDED.analysis_json, reviewer = EXCLUDED.reviewer, \
             reviewed_at = EXCLUDED.reviewed_at, review_note = EXCLUDED.review_note, disposition = EXCLUDED.disposition, \
             zone_key = EXCLUDED.zone_key, association_key = EXCLUDED.association_key",
        )
        .bind(event.id)
        .bind(event.job_id)
        .bind(&event.event_type)
        .bind(event.start_time_ms as i64)
        .bind(event.end_time_ms as i64)
        .bind(&event.severity)
        .bind(event_status_name(&event.status))
        .bind(event.confidence)
        .bind(Json(&event.objects))
        .bind(Json(&event.evidence))
        .bind(event.analysis.as_ref().map(Json))
        .bind(&event.rule_version)
        .bind(&event.prompt_version)
        .bind(&event.detector_version)
        .bind(&event.reviewer)
        .bind(&event.reviewed_at)
        .bind(&event.review_note)
        .bind(&event.disposition)
        .bind(&event.zone_key)
        .bind(&event.association_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_events(&self) -> Result<Vec<Event>, sqlx::Error> {
        self.list_events_limited(200).await
    }

    pub async fn list_events_limited(&self, limit: usize) -> Result<Vec<Event>, sqlx::Error> {
        let limit = limit.clamp(1, 200);
        let rows = sqlx::query("SELECT e.id, e.job_id, e.event_type, e.start_time_ms, e.end_time_ms, e.severity, e.status, e.confidence, e.objects_json, e.evidence_json, e.analysis_json, e.rule_version, e.prompt_version, e.detector_version, e.reviewer, EXTRACT(EPOCH FROM e.reviewed_at)::BIGINT::TEXT AS reviewed_at, e.review_note, e.disposition, e.zone_key, e.association_key FROM events e INNER JOIN video_jobs j ON j.id = e.job_id WHERE j.deleted_at IS NULL ORDER BY e.created_at DESC LIMIT $1")
            .bind(limit as i64)
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| event_from_row(&row))
            .collect())
    }

    pub async fn list_events_all(&self) -> Result<Vec<Event>, sqlx::Error> {
        let rows = sqlx::query("SELECT e.id, e.job_id, e.event_type, e.start_time_ms, e.end_time_ms, e.severity, e.status, e.confidence, e.objects_json, e.evidence_json, e.analysis_json, e.rule_version, e.prompt_version, e.detector_version, e.reviewer, EXTRACT(EPOCH FROM e.reviewed_at)::BIGINT::TEXT AS reviewed_at, e.review_note, e.disposition, e.zone_key, e.association_key FROM events e INNER JOIN video_jobs j ON j.id = e.job_id WHERE j.deleted_at IS NULL ORDER BY e.created_at DESC")
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| event_from_row(&row))
            .collect())
    }

    pub async fn get_event(&self, id: Uuid) -> Result<Option<Event>, sqlx::Error> {
        let row = sqlx::query("SELECT e.id, e.job_id, e.event_type, e.start_time_ms, e.end_time_ms, e.severity, e.status, e.confidence, e.objects_json, e.evidence_json, e.analysis_json, e.rule_version, e.prompt_version, e.detector_version, e.reviewer, EXTRACT(EPOCH FROM e.reviewed_at)::BIGINT::TEXT AS reviewed_at, e.review_note, e.disposition, e.zone_key, e.association_key FROM events e INNER JOIN video_jobs j ON j.id = e.job_id WHERE e.id = $1 AND j.deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().and_then(event_from_row))
    }

    pub async fn update_event_status(
        &self,
        id: Uuid,
        status: EventStatus,
    ) -> Result<Option<Event>, sqlx::Error> {
        let result = sqlx::query("UPDATE events SET status = $1 WHERE id = $2")
            .bind(event_status_name(&status))
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_event(id).await
    }

    pub async fn save_review(&self, review: &EventReview) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO event_review_history (id, event_id, old_status, new_status, reviewer, note, disposition) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(review.id)
            .bind(review.event_id)
            .bind(event_status_name(&review.old_status))
            .bind(event_status_name(&review.new_status))
            .bind(&review.reviewer)
            .bind(&review.note)
            .bind(&review.disposition)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_reviews(&self, event_id: Uuid) -> Result<Vec<EventReview>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, event_id, old_status, new_status, reviewer, note, disposition, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at FROM event_review_history WHERE event_id = $1 ORDER BY created_at ASC")
            .bind(event_id).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(EventReview {
                    id: row.try_get("id").ok()?,
                    event_id: row.try_get("event_id").ok()?,
                    old_status: event_status_from_name(
                        &row.try_get::<String, _>("old_status").ok()?,
                    ),
                    new_status: event_status_from_name(
                        &row.try_get::<String, _>("new_status").ok()?,
                    ),
                    reviewer: row.try_get("reviewer").ok()?,
                    note: row.try_get("note").ok()?,
                    disposition: row.try_get("disposition").ok()?,
                    created_at: row.try_get("created_at").ok()?,
                })
            })
            .collect())
    }

    pub async fn delete_event(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM events WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_rules(&self) -> Result<Vec<EventRule>, sqlx::Error> {
        let rows = sqlx::query("SELECT event_type, class_name, min_confidence, min_duration_ms, version, geometry_json, threshold_value, enabled FROM event_rules")
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(EventRule {
                    event_type: row.try_get("event_type").ok()?,
                    class_name: row.try_get("class_name").ok()?,
                    min_confidence: row.try_get("min_confidence").ok()?,
                    min_duration_ms: u64::try_from(row.try_get::<i64, _>("min_duration_ms").ok()?)
                        .ok()?,
                    version: row.try_get("version").ok()?,
                    geometry: row
                        .try_get::<Option<Json<_>>, _>("geometry_json")
                        .ok()?
                        .map(|value| value.0),
                    threshold: row
                        .try_get::<Option<i64>, _>("threshold_value")
                        .ok()?
                        .and_then(|value| u32::try_from(value).ok()),
                    enabled: row.try_get("enabled").ok()?,
                })
            })
            .collect())
    }

    pub async fn save_rule(&self, rule: &EventRule) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO event_rules (event_type, class_name, min_confidence, min_duration_ms, version, geometry_json, threshold_value, enabled) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (event_type) DO UPDATE SET \
             class_name = EXCLUDED.class_name, min_confidence = EXCLUDED.min_confidence, \
             min_duration_ms = EXCLUDED.min_duration_ms, version = EXCLUDED.version, \
             geometry_json = EXCLUDED.geometry_json, threshold_value = EXCLUDED.threshold_value, \
             enabled = EXCLUDED.enabled",
        )
            .bind(&rule.event_type)
            .bind(&rule.class_name)
            .bind(rule.min_confidence)
            .bind(rule.min_duration_ms as i64)
            .bind(&rule.version)
            .bind(rule.geometry.as_ref().map(Json))
            .bind(rule.threshold.map(i64::from))
            .bind(rule.enabled)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn status_name(status: &JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "pending",
        JobStatus::Processing => "processing",
        JobStatus::Completed => "completed",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

fn stage_name(stage: &JobStage) -> &'static str {
    match stage {
        JobStage::Preparing => "preparing",
        JobStage::Reading => "reading",
        JobStage::ExtractingFrames => "extracting_frames",
        JobStage::Detecting => "detecting",
        JobStage::AnalyzingEvents => "analyzing_events",
        JobStage::GeneratingPlayback => "generating_playback",
        JobStage::Finalizing => "finalizing",
    }
}

fn stage_from_name(stage: &str) -> Option<JobStage> {
    match stage {
        "preparing" => Some(JobStage::Preparing),
        "reading" => Some(JobStage::Reading),
        "extracting_frames" => Some(JobStage::ExtractingFrames),
        "detecting" => Some(JobStage::Detecting),
        "analyzing_events" => Some(JobStage::AnalyzingEvents),
        "generating_playback" => Some(JobStage::GeneratingPlayback),
        "finalizing" => Some(JobStage::Finalizing),
        _ => None,
    }
}

fn job_from_row(row: &PgRow) -> Option<VideoJob> {
    Some(VideoJob {
        id: row.try_get("id").ok()?,
        filename: row.try_get("filename").ok()?,
        duration_ms: u64::try_from(row.try_get::<i64, _>("duration_ms").ok()?).ok()?,
        status: match row.try_get::<String, _>("status").ok()?.as_str() {
            "processing" => JobStatus::Processing,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        },
        progress: u8::try_from(row.try_get::<i16, _>("progress").ok()?).ok()?,
        source_uri: row.try_get("source_uri").ok()?,
        annotated_video_url: row.try_get("annotated_video_url").ok()?,
        annotated_video_status: row.try_get("annotated_video_status").ok()?,
        annotated_video_error: row.try_get("annotated_video_error").ok()?,
        stage: row
            .try_get::<Option<String>, _>("progress_stage")
            .ok()?
            .as_deref()
            .and_then(stage_from_name),
        status_message: row.try_get("status_message").ok()?,
        attempt: u32::try_from(row.try_get::<i64, _>("attempt").ok()?).ok()?,
        created_at: row
            .try_get::<Option<i64>, _>("created_at_ms")
            .ok()?
            .and_then(|value| u64::try_from(value).ok()),
        updated_at: row
            .try_get::<Option<i64>, _>("updated_at_ms")
            .ok()?
            .and_then(|value| u64::try_from(value).ok()),
    })
}

fn event_status_name(status: &EventStatus) -> &'static str {
    match status {
        EventStatus::Unreviewed => "unreviewed",
        EventStatus::Confirmed => "confirmed",
        EventStatus::Ignored => "ignored",
        EventStatus::Processing => "processing",
        EventStatus::Resolved => "resolved",
        EventStatus::Closed => "closed",
    }
}
fn event_status_from_name(status: &str) -> EventStatus {
    match status {
        "confirmed" => EventStatus::Confirmed,
        "ignored" => EventStatus::Ignored,
        "processing" => EventStatus::Processing,
        "resolved" => EventStatus::Resolved,
        "closed" => EventStatus::Closed,
        _ => EventStatus::Unreviewed,
    }
}
fn event_from_row(row: &PgRow) -> Option<Event> {
    Some(Event {
        id: row.try_get("id").ok()?,
        job_id: row.try_get("job_id").ok()?,
        event_type: row.try_get("event_type").ok()?,
        start_time_ms: u64::try_from(row.try_get::<i64, _>("start_time_ms").ok()?).ok()?,
        end_time_ms: u64::try_from(row.try_get::<i64, _>("end_time_ms").ok()?).ok()?,
        severity: row.try_get("severity").ok()?,
        status: match row.try_get::<String, _>("status").ok()?.as_str() {
            "confirmed" => EventStatus::Confirmed,
            "ignored" => EventStatus::Ignored,
            "processing" => EventStatus::Processing,
            "resolved" => EventStatus::Resolved,
            "closed" => EventStatus::Closed,
            _ => EventStatus::Unreviewed,
        },
        confidence: row.try_get("confidence").ok()?,
        objects: row.try_get::<Json<_>, _>("objects_json").ok()?.0,
        evidence: row.try_get::<Json<_>, _>("evidence_json").ok()?.0,
        analysis: row
            .try_get::<Option<Json<_>>, _>("analysis_json")
            .ok()?
            .map(|value| value.0),
        rule_version: row.try_get("rule_version").ok()?,
        prompt_version: row.try_get("prompt_version").ok()?,
        detector_version: row.try_get("detector_version").ok()?,
        reviewer: row.try_get("reviewer").ok()?,
        reviewed_at: row.try_get("reviewed_at").ok()?,
        review_note: row.try_get("review_note").ok()?,
        disposition: row.try_get("disposition").ok()?,
        zone_key: row.try_get("zone_key").ok()?,
        association_key: row.try_get("association_key").ok()?,
        related_event_ids: Vec::new(),
    })
}
