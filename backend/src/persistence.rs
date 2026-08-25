use crate::domain::{Event, EventStatus, JobStatus, VideoJob};
use sqlx::{mysql::MySqlPoolOptions, MySqlPool, Row};
use sqlx::types::Json;
use uuid::Uuid;

#[derive(Debug)]
pub enum JobMutationError {
    NotFound,
    Conflict,
    Database(sqlx::Error),
}

#[derive(Clone)]
pub struct Database {
    pub pool: MySqlPool,
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
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .connect(&config.url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("../db/migrations").run(&self.pool).await
    }

    pub async fn save_job(&self, job: &VideoJob) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO video_jobs (id, filename, duration_ms, status, progress, source_uri) VALUES (?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE status=VALUES(status), progress=VALUES(progress), source_uri=VALUES(source_uri)")
            .bind(job.id.to_string()).bind(&job.filename).bind(job.duration_ms as i64).bind(status_name(&job.status)).bind(job.progress as i32).bind(&job.source_uri).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Option<VideoJob>, sqlx::Error> {
        let row = sqlx::query("SELECT id, filename, duration_ms, status, progress, source_uri FROM video_jobs WHERE id = ? AND deleted_at IS NULL")
            .bind(id.to_string()).fetch_optional(&self.pool).await?;
        Ok(row.and_then(|row| {
            Some(VideoJob {
                id: Uuid::parse_str(&row.try_get::<String, _>("id").ok()?).ok()?,
                filename: row.try_get("filename").ok()?,
                duration_ms: row.try_get::<u64, _>("duration_ms").ok()?,
                status: match row.try_get::<String, _>("status").ok()?.as_str() {
                    "processing" => JobStatus::Processing,
                    "completed" => JobStatus::Completed,
                    "failed" => JobStatus::Failed,
                    "cancelled" => JobStatus::Cancelled,
                    _ => JobStatus::Pending,
                },
                progress: row.try_get::<u8, _>("progress").ok()?,
                source_uri: row.try_get("source_uri").ok()?,
            })
        }))
    }

    pub async fn update_job_filename(
        &self,
        id: Uuid,
        filename: &str,
    ) -> Result<Option<VideoJob>, sqlx::Error> {
        let result = sqlx::query("UPDATE video_jobs SET filename = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(filename)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_job(id).await
    }

    pub async fn soft_delete_job(&self, id: Uuid) -> Result<(), JobMutationError> {
        let mut transaction = self.pool.begin().await.map_err(JobMutationError::Database)?;
        let row = sqlx::query("SELECT status FROM video_jobs WHERE id = ? AND deleted_at IS NULL FOR UPDATE")
            .bind(id.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(JobMutationError::Database)?;
        let Some(row) = row else { return Err(JobMutationError::NotFound); };
        let status: String = row.try_get("status").map_err(JobMutationError::Database)?;
        if status == "processing" {
            return Err(JobMutationError::Conflict);
        }
        sqlx::query("DELETE FROM events WHERE job_id = ?")
            .bind(id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(JobMutationError::Database)?;
        sqlx::query("UPDATE video_jobs SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(JobMutationError::Database)?;
        transaction.commit().await.map_err(JobMutationError::Database)
    }

    pub async fn list_jobs(&self) -> Result<Vec<VideoJob>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, filename, duration_ms, status, progress, source_uri FROM video_jobs WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 200")
            .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().filter_map(|row| {
            Some(VideoJob {
                id: Uuid::parse_str(&row.try_get::<String, _>("id").ok()?).ok()?,
                filename: row.try_get("filename").ok()?,
                duration_ms: row.try_get::<u64, _>("duration_ms").ok()?,
                status: match row.try_get::<String, _>("status").ok()?.as_str() {
                    "processing" => JobStatus::Processing,
                    "completed" => JobStatus::Completed,
                    "failed" => JobStatus::Failed,
                    "cancelled" => JobStatus::Cancelled,
                    _ => JobStatus::Pending,
                },
                progress: row.try_get::<u8, _>("progress").ok()?,
                source_uri: row.try_get("source_uri").ok()?,
            })
        }).collect())
    }

    pub async fn save_event(&self, event: &Event) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO events (id, job_id, event_type, start_time_ms, end_time_ms, severity, status, confidence, objects_json, evidence_json, analysis_json, rule_version, prompt_version, detector_version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE status=VALUES(status), analysis_json=VALUES(analysis_json)")
            .bind(event.id.to_string()).bind(event.job_id.to_string()).bind(&event.event_type).bind(event.start_time_ms as i64).bind(event.end_time_ms as i64).bind(&event.severity).bind(event_status_name(&event.status)).bind(event.confidence).bind(serde_json::to_string(&event.objects).unwrap_or_default()).bind(serde_json::to_string(&event.evidence).unwrap_or_default()).bind(serde_json::to_string(&event.analysis).unwrap_or_default()).bind(&event.rule_version).bind(&event.prompt_version).bind(&event.detector_version).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_events(&self) -> Result<Vec<Event>, sqlx::Error> {
        let rows = sqlx::query("SELECT e.id, e.job_id, e.event_type, e.start_time_ms, e.end_time_ms, e.severity, e.status, e.confidence, e.objects_json, e.evidence_json, e.analysis_json, e.rule_version, e.prompt_version, e.detector_version FROM events e INNER JOIN video_jobs j ON j.id = e.job_id WHERE j.deleted_at IS NULL ORDER BY e.created_at DESC LIMIT 200").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| event_from_row(&row))
            .collect())
    }

    pub async fn get_event(&self, id: Uuid) -> Result<Option<Event>, sqlx::Error> {
        let row = sqlx::query("SELECT e.id, e.job_id, e.event_type, e.start_time_ms, e.end_time_ms, e.severity, e.status, e.confidence, e.objects_json, e.evidence_json, e.analysis_json, e.rule_version, e.prompt_version, e.detector_version FROM events e INNER JOIN video_jobs j ON j.id = e.job_id WHERE e.id = ? AND j.deleted_at IS NULL")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().and_then(event_from_row))
    }

    pub async fn update_event_status(
        &self,
        id: Uuid,
        status: EventStatus,
    ) -> Result<Option<Event>, sqlx::Error> {
        let result = sqlx::query("UPDATE events SET status = ? WHERE id = ?")
            .bind(event_status_name(&status))
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_event(id).await
    }

    pub async fn delete_event(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
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
fn event_status_name(status: &EventStatus) -> &'static str {
    match status {
        EventStatus::Unreviewed => "unreviewed",
        EventStatus::Confirmed => "confirmed",
        EventStatus::Ignored => "ignored",
    }
}
fn event_from_row(row: &sqlx::mysql::MySqlRow) -> Option<Event> {
    Some(Event {
        id: Uuid::parse_str(&row.try_get::<String, _>("id").ok()?).ok()?,
        job_id: Uuid::parse_str(&row.try_get::<String, _>("job_id").ok()?).ok()?,
        event_type: row.try_get("event_type").ok()?,
        start_time_ms: row.try_get::<u64, _>("start_time_ms").ok()?,
        end_time_ms: row.try_get::<u64, _>("end_time_ms").ok()?,
        severity: row.try_get("severity").ok()?,
        status: match row.try_get::<String, _>("status").ok()?.as_str() {
            "confirmed" => EventStatus::Confirmed,
            "ignored" => EventStatus::Ignored,
            _ => EventStatus::Unreviewed,
        },
        confidence: row.try_get("confidence").ok()?,
        objects: row.try_get::<Json<_>, _>("objects_json").ok()?.0,
        evidence: row.try_get::<Json<_>, _>("evidence_json").ok()?.0,
        analysis: row
            .try_get::<Json<Option<_>>, _>("analysis_json")
            .ok()?
            .0,
        rule_version: row.try_get("rule_version").ok()?,
        prompt_version: row.try_get("prompt_version").ok()?,
        detector_version: row.try_get("detector_version").ok()?,
    })
}
