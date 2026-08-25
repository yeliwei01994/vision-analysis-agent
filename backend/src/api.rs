use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header::{CACHE_CONTROL, CONTENT_TYPE}, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use std::path::{Component, PathBuf};
use uuid::Uuid;

use crate::{
    application::AppState,
    domain::{Event, EventStatus, VideoJob},
    rules::{EventRule, Geometry},
};

#[derive(Debug, Deserialize)]
pub struct UpdateVideoRequest {
    pub filename: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateVideoRequest {
    pub filename: String,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub class_name: String,
    pub min_confidence: f32,
    pub min_duration_ms: u64,
    pub geometry: Option<Geometry>,
    pub threshold: Option<u32>,
    #[serde(default = "default_rule_enabled")]
    pub enabled: bool,
}

fn default_rule_enabled() -> bool { true }

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/media/*path", get(get_media))
        .route("/api/v1/videos", post(create_video))
        .route("/api/v1/videos/upload", post(upload_video))
        .route("/api/v1/videos/:id/process", post(process_video))
        .route("/api/v1/jobs", get(list_jobs))
        .route("/api/v1/jobs/:id", get(get_job))
        .route("/api/v1/jobs/:id", put(update_job).delete(delete_job))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/events/:id/confirm", post(confirm_event))
        .route("/api/v1/events/:id/ignore", post(ignore_event))
        .route("/api/v1/event-rules", get(list_rules))
        .route("/api/v1/event-rules/:event_type", put(update_rule))
        // `search` is a POST on the same dynamic path so Axum's router does
        // not need two overlapping static/dynamic route patterns.
        .route("/api/v1/events/:id", get(get_event).post(search_events).delete(delete_event))
        .layer(DefaultBodyLimit::max(500 * 1024 * 1024))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok","service":"vision-event-api"}))
}

async fn get_media(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, ApiError> {
    let relative = safe_media_path(&path).ok_or(ApiError::NotFound)?;
    let bytes = tokio::fs::read(state.storage.root().join(relative))
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(([(CONTENT_TYPE, "image/jpeg"), (CACHE_CONTROL, "private, max-age=3600")], bytes).into_response())
}

fn safe_media_path(value: &str) -> Option<PathBuf> {
    let path = std::path::Path::new(value);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            _ => return None,
        }
    }
    (!safe.as_os_str().is_empty()).then_some(safe)
}

async fn create_video(
    State(state): State<AppState>,
    Json(input): Json<CreateVideoRequest>,
) -> Json<VideoJob> {
    let job = state.create_job(input.filename, input.duration_ms.unwrap_or(0));
    let event = state.seed_event(&job);
    state.complete_job(job.id);
    if let Some(database) = &state.database {
        let _ = database
            .save_job(&state.job(job.id).unwrap_or(job.clone()))
            .await;
        let _ = database.save_event(&event).await;
    }
    Json(state.job(job.id).unwrap_or(job))
}

async fn upload_video(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<VideoJob>, ApiError> {
    let mut filename = "upload.bin".to_string();
    let mut bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest)?
    {
        if field.name() == Some("file") {
            if let Some(name) = field.file_name() {
                filename = name.to_string();
            }
            bytes = Some(field.bytes().await.map_err(|_| ApiError::BadRequest)?);
            break;
        }
    }
    let bytes = bytes.ok_or(ApiError::BadRequest)?;
    let saved = state
        .storage
        .save_upload(&filename, &bytes)
        .await
        .map_err(|_| ApiError::Internal)?;
    let metadata = crate::video::probe(&saved).await;
    let mut job = state.create_job(filename, metadata.duration_ms);
    job.source_uri = Some(saved.to_string_lossy().to_string());
    state
        .jobs
        .write()
        .expect("jobs lock poisoned")
        .insert(job.id, job.clone());
    if let Some(database) = &state.database {
        let _ = database.save_job(&job).await;
    }
    if let Some(queue) = &state.queue {
        state.update_job(job.id, crate::domain::JobStatus::Processing, 1);
        queue
            .enqueue(&crate::queue::QueueMessage::new(job.id))
            .await
            .map_err(|_| ApiError::Internal)?;
    }
    Ok(Json(state.job(job.id).unwrap_or(job)))
}

async fn process_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoJob>, ApiError> {
    if let Some(queue) = &state.queue {
        if state.job(id).is_none() {
            return Err(ApiError::NotFound);
        }
        if matches!(
            state.job(id).map(|job| job.status),
            Some(crate::domain::JobStatus::Processing | crate::domain::JobStatus::Completed)
        ) {
            return state.job(id).map(Json).ok_or(ApiError::NotFound);
        }
        state.update_job(id, crate::domain::JobStatus::Processing, 1);
        queue
            .enqueue(&crate::queue::QueueMessage::new(id))
            .await
            .map_err(|_| ApiError::Internal)?;
        if let Some(database) = &state.database {
            if let Some(job) = state.job(id) {
                let _ = database.save_job(&job).await;
            }
        }
        return state.job(id).map(Json).ok_or(ApiError::NotFound);
    }
    if !crate::worker::process_job(state.clone(), id).await {
        return state.job(id).map(Json).ok_or(ApiError::NotFound);
    }
    if let Some(database) = &state.database {
        if let Some(job) = state.job(id) {
            let _ = database.save_job(&job).await;
        }
    }
    state.job(id).map(Json).ok_or(ApiError::NotFound)
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoJob>, ApiError> {
    if let Some(database) = &state.database {
        match database.get_job(id).await {
            Ok(Some(job)) => {
                state
                    .jobs
                    .write()
                    .expect("jobs lock poisoned")
                    .insert(job.id, job.clone());
                return Ok(Json(job));
            }
            Ok(None) => {}
            Err(error) => eprintln!("failed to load job {id} from MySQL: {error}"),
        }
    }
    if let Some(job) = state.job(id) {
        return Ok(Json(job));
    }
    Err(ApiError::NotFound)
}

async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateVideoRequest>,
) -> Result<Json<VideoJob>, ApiError> {
    let filename = input.filename.trim();
    if filename.is_empty() || filename.chars().count() > 255 {
        return Err(ApiError::BadRequest);
    }
    if let Some(database) = &state.database {
        let job = database
            .update_job_filename(id, filename)
            .await
            .map_err(|_| ApiError::Internal)?
            .ok_or(ApiError::NotFound)?;
        state.jobs.write().expect("jobs lock poisoned").insert(id, job.clone());
        return Ok(Json(job));
    }
    state.update_job_filename(id, filename.to_string()).map(Json).ok_or(ApiError::NotFound)
}

async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if let Some(database) = &state.database {
        database.soft_delete_job(id).await.map_err(|error| match error {
            crate::persistence::JobMutationError::NotFound => ApiError::NotFound,
            crate::persistence::JobMutationError::Conflict => ApiError::Conflict,
            crate::persistence::JobMutationError::Database(_) => ApiError::Internal,
        })?;
        state.forget_job(id);
        return Ok(StatusCode::NO_CONTENT);
    }
    match state.delete_job(id) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(crate::domain::JobStatus::Processing) => Err(ApiError::Conflict),
        Err(_) => Err(ApiError::NotFound),
    }
}
async fn list_jobs(State(state): State<AppState>) -> Json<Vec<VideoJob>> {
    if let Some(database) = &state.database {
        if let Ok(jobs) = database.list_jobs().await {
            return Json(jobs);
        }
    }
    Json(state.jobs())
}
async fn list_events(State(state): State<AppState>) -> Json<Vec<Event>> {
    if let Some(database) = &state.database {
        match database.list_events().await {
            Ok(events) => {
                println!("loaded {} events from MySQL", events.len());
                return Json(events);
            }
            Err(error) => eprintln!("failed to list events from MySQL: {error}"),
        }
    }
    Json(state.events())
}
async fn list_rules(State(state): State<AppState>) -> Json<Vec<EventRule>> {
    Json(state.event_rules())
}
async fn update_rule(
    State(state): State<AppState>,
    Path(event_type): Path<String>,
    Json(input): Json<UpdateRuleRequest>,
) -> Result<Json<EventRule>, ApiError> {
    if !(0.0..=1.0).contains(&input.min_confidence) {
        return Err(ApiError::BadRequest);
    }
    if input.geometry.as_ref().is_some_and(|geometry| geometry.kind != "polygon" || geometry.points.len() < 3 || geometry.points.iter().flatten().any(|value| !(0.0..=1.0).contains(value))) {
        return Err(ApiError::BadRequest);
    }
    if event_type == "person_count_limit" && input.threshold.unwrap_or(0) == 0 {
        return Err(ApiError::BadRequest);
    }
    let mut rule = EventRule::new(
        event_type.clone(),
        input.class_name,
        input.min_confidence,
        input.min_duration_ms,
    );
    rule.geometry = input.geometry;
    rule.threshold = input.threshold;
    rule.enabled = input.enabled;
    let updated = state.update_rule(event_type, rule);
    if let Some(database) = &state.database {
        database.save_rule(&updated).await.map_err(|_| ApiError::Internal)?;
    }
    Ok(Json(updated))
}
async fn get_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    if let Some(database) = &state.database {
        let event = database
            .get_event(id)
            .await
            .map_err(|_| ApiError::Internal)?
            .ok_or(ApiError::NotFound)?;
        state.events.write().expect("events lock poisoned").insert(event.id, event.clone());
        return Ok(Json(event));
    }
    state.event(id).map(Json).ok_or(ApiError::NotFound)
}

async fn confirm_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    review_event(state, id, EventStatus::Confirmed).await
}

async fn ignore_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    review_event(state, id, EventStatus::Ignored).await
}

async fn delete_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if let Some(database) = &state.database {
        if !database.delete_event(id).await.map_err(|_| ApiError::Internal)? {
            return Err(ApiError::NotFound);
        }
    } else if state.event(id).is_none() {
        return Err(ApiError::NotFound);
    }
    state.events.write().expect("events lock poisoned").remove(&id);
    if let Err(error) = state.storage.delete_event_evidence(id).await {
        eprintln!("failed to delete evidence for event {id}: {error}");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn review_event(
    state: AppState,
    id: Uuid,
    status: EventStatus,
) -> Result<Json<Event>, ApiError> {
    if let Some(database) = &state.database {
        let event = database
            .update_event_status(id, status)
            .await
            .map_err(|_| ApiError::Internal)?
            .ok_or(ApiError::NotFound)?;
        state.events.write().expect("events lock poisoned").insert(event.id, event.clone());
        return Ok(Json(event));
    }
    let event = state.review_event(id, status).ok_or(ApiError::NotFound)?;
    Ok(Json(event))
}

async fn search_events(
    State(state): State<AppState>,
    Path(_search_key): Path<String>,
    Json(input): Json<serde_json::Value>,
) -> Json<Vec<Event>> {
    let keyword = input.get("keyword").and_then(|v| v.as_str()).unwrap_or("");
    Json(
        state
            .events()
            .into_iter()
            .filter(|event| keyword.is_empty() || event.event_type.contains(keyword))
            .collect(),
    )
}

enum ApiError {
    NotFound,
    BadRequest,
    Conflict,
    Internal,
}
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Conflict => (StatusCode::CONFLICT, "conflict"),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        (status, Json(serde_json::json!({"error": code}))).into_response()
    }
}
