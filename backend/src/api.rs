use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    application::AppState,
    domain::{Event, EventStatus, VideoJob},
    rules::EventRule,
};

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
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/videos", post(create_video))
        .route("/api/v1/videos/upload", post(upload_video))
        .route("/api/v1/videos/:id/process", post(process_video))
        .route("/api/v1/jobs/:id", get(get_job))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/events/:id/confirm", post(confirm_event))
        .route("/api/v1/events/:id/ignore", post(ignore_event))
        .route("/api/v1/event-rules", get(list_rules))
        .route("/api/v1/event-rules/:event_type", put(update_rule))
        // `search` is a POST on the same dynamic path so Axum's router does
        // not need two overlapping static/dynamic route patterns.
        .route("/api/v1/events/:id", get(get_event).post(search_events))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok","service":"vision-event-api"}))
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
    Ok(Json(job))
}

async fn process_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoJob>, ApiError> {
    if let Some(queue) = &state.queue {
        if state.job(id).is_none() {
            return Err(ApiError::NotFound);
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
        return Err(ApiError::NotFound);
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
    state.job(id).map(Json).ok_or(ApiError::NotFound)
}
async fn list_events(State(state): State<AppState>) -> Json<Vec<Event>> {
    if let Some(database) = &state.database {
        if let Ok(events) = database.list_events().await {
            return Json(events);
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
    let rule = EventRule::new(
        event_type.clone(),
        input.class_name,
        input.min_confidence,
        input.min_duration_ms,
    );
    Ok(Json(state.update_rule(event_type, rule)))
}
async fn get_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
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

async fn review_event(
    state: AppState,
    id: Uuid,
    status: EventStatus,
) -> Result<Json<Event>, ApiError> {
    let event = state.review_event(id, status).ok_or(ApiError::NotFound)?;
    if let Some(database) = &state.database {
        database
            .save_event(&event)
            .await
            .map_err(|_| ApiError::Internal)?;
    }
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
    Internal,
}
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        (status, Json(serde_json::json!({"error": code}))).into_response()
    }
}
