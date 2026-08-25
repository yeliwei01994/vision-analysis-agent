use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header::{CACHE_CONTROL, CONTENT_TYPE}, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Component, PathBuf};
use uuid::Uuid;

use crate::{
    application::AppState,
    domain::{Event, EventReview, EventStatus, VideoJob},
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

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub status: EventStatus,
    pub reviewer: Option<String>,
    pub note: Option<String>,
    pub disposition: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct EventQuery {
    pub job_id: Option<Uuid>, pub event_type: Option<String>, pub zone_key: Option<String>, pub class_name: Option<String>, pub status: Option<EventStatus>, pub severity: Option<String>, pub min_confidence: Option<f32>, pub max_confidence: Option<f32>, pub from_ms: Option<u64>, pub to_ms: Option<u64>, pub reviewer: Option<String>, pub page: Option<usize>, pub page_size: Option<usize>, pub sort: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventPage { pub items: Vec<Event>, pub total: usize, pub page: usize, pub page_size: usize }

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
        .route("/api/v1/events/query", get(query_events))
        .route("/api/v1/events/export.csv", get(export_events))
        .route("/api/v1/events/:id/confirm", post(confirm_event))
        .route("/api/v1/events/:id/ignore", post(ignore_event))
        .route("/api/v1/events/:id/review", post(review_event_request))
        .route("/api/v1/events/:id/reviews", get(list_event_reviews))
        .route("/api/v1/events/:id/report.html", get(report_event))
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
                return Json(with_related(events));
            }
            Err(error) => eprintln!("failed to list events from MySQL: {error}"),
        }
    }
    Json(with_related(state.events()))
}

fn with_related(mut events: Vec<Event>) -> Vec<Event> {
    let mut groups: HashMap<String, Vec<Uuid>> = HashMap::new();
    for event in &events { if let Some(key) = &event.association_key { groups.entry(key.clone()).or_default().push(event.id); } }
    for event in &mut events { if let Some(key) = &event.association_key { event.related_event_ids = groups.get(key).cloned().unwrap_or_default().into_iter().filter(|id| *id != event.id).collect(); } }
    events
}

async fn query_events(State(state): State<AppState>, Query(query): Query<EventQuery>) -> Json<EventPage> {
    let mut events = if let Some(database) = &state.database { database.list_events_all().await.unwrap_or_default() } else { state.events() };
    events.retain(|event| query.job_id.map_or(true, |value| event.job_id == value));
    events.retain(|event| query.event_type.as_ref().map_or(true, |value| event.event_type == *value));
    events.retain(|event| query.zone_key.as_ref().map_or(true, |value| event.zone_key.as_deref() == Some(value.as_str())));
    events.retain(|event| query.class_name.as_ref().map_or(true, |value| event.objects.iter().any(|object| object.class_name == *value)));
    events.retain(|event| query.status.as_ref().map_or(true, |value| event.status == *value));
    events.retain(|event| query.severity.as_ref().map_or(true, |value| event.severity == *value));
    events.retain(|event| query.min_confidence.map_or(true, |value| event.confidence >= value));
    events.retain(|event| query.max_confidence.map_or(true, |value| event.confidence <= value));
    events.retain(|event| query.from_ms.map_or(true, |value| event.end_time_ms >= value));
    events.retain(|event| query.to_ms.map_or(true, |value| event.start_time_ms <= value));
    events.retain(|event| query.reviewer.as_ref().map_or(true, |value| event.reviewer.as_deref() == Some(value.as_str())));
    if query.sort.as_deref() == Some("oldest") { events.sort_by_key(|event| event.start_time_ms); } else { events.sort_by_key(|event| std::cmp::Reverse(event.start_time_ms)); }
    let total = events.len(); let page = query.page.unwrap_or(1).max(1); let page_size = query.page_size.unwrap_or(25).clamp(1, 100); let start = (page - 1) * page_size;
    let events = with_related(events);
    let items = events.into_iter().skip(start).take(page_size).collect();
    Json(EventPage { items, total, page, page_size })
}

async fn export_events(State(state): State<AppState>, Query(query): Query<EventQuery>) -> Response {
    let page = query_events(State(state), Query(query)).await.0;
    let mut csv = String::from("id,job_id,event_type,zone_key,status,severity,confidence,start_time_ms,end_time_ms,reviewer\n");
    for event in page.items { csv.push_str(&format!("{},{},{},{},{},{},{:.4},{},{},{}\n", event.id, event.job_id, csv_cell(&event.event_type), csv_cell(event.zone_key.as_deref().unwrap_or("")), csv_cell(&format!("{:?}", event.status).to_lowercase()), csv_cell(&event.severity), event.confidence, event.start_time_ms, event.end_time_ms, csv_cell(event.reviewer.as_deref().unwrap_or("")))); }
    ([(CONTENT_TYPE, "text/csv; charset=utf-8"), (CACHE_CONTROL, "no-store")], format!("\u{feff}{csv}")).into_response()
}

fn csv_cell(value: &str) -> String { if value.contains([',', '"', '\n']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value.to_string() } }
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

async fn report_event(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Response, ApiError> {
    let event = if let Some(database) = &state.database { database.get_event(id).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::NotFound)? } else { state.event(id).ok_or(ApiError::NotFound)? };
    let images = event.evidence.frames.iter().map(|frame| format!("<figure><img src=\"{}\" alt=\"{}ms\"><figcaption>{} ms</figcaption></figure>", html_escape(&frame.image_url), frame.timestamp_ms, frame.timestamp_ms)).collect::<String>();
    let body = format!("<!doctype html><meta charset=\"utf-8\"><title>事件报告 {id}</title><style>body{{font-family:Arial;background:#101820;color:#e6f0f5;padding:32px}}img{{max-width:480px;margin:8px}}figure{{display:inline-block}}</style><h1>事件报告</h1><p>事件：{} · 状态：{:?} · 时间：{}–{} ms</p><p>规则：{} · 置信度：{:.1}%</p><p>审核：{} {} {}</p><h2>证据</h2>{images}", html_escape(&event.event_type), event.status, event.start_time_ms, event.end_time_ms, html_escape(&event.rule_version), event.confidence * 100.0, html_escape(event.reviewer.as_deref().unwrap_or("未审核")), html_escape(event.disposition.as_deref().unwrap_or("")), html_escape(event.review_note.as_deref().unwrap_or("")));
    Ok(([(CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response())
}

fn html_escape(value: &str) -> String { value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

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

async fn review_event_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReviewRequest>,
) -> Result<Json<Event>, ApiError> {
    let current = if let Some(database) = &state.database { database.get_event(id).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::NotFound)? } else { state.event(id).ok_or(ApiError::NotFound)? };
    if !valid_transition(&current.status, &input.status) { return Err(ApiError::Conflict); }
    let mut updated = current.clone();
    updated.status = input.status.clone();
    updated.reviewer = input.reviewer.clone();
    updated.review_note = input.note.clone();
    updated.disposition = input.disposition.clone();
    updated.reviewed_at = Some(chrono_like_now());
    let review = EventReview { id: Uuid::new_v4(), event_id: id, old_status: current.status, new_status: input.status, reviewer: input.reviewer, note: input.note, disposition: input.disposition, created_at: chrono_like_now() };
    if let Some(database) = &state.database { database.save_event(&updated).await.map_err(|_| ApiError::Internal)?; database.save_review(&review).await.map_err(|_| ApiError::Internal)?; }
    state.events.write().expect("events lock poisoned").insert(id, updated.clone());
    Ok(Json(updated))
}

async fn list_event_reviews(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<Vec<EventReview>>, ApiError> {
    if let Some(database) = &state.database { return Ok(Json(database.list_reviews(id).await.map_err(|_| ApiError::Internal)?)); }
    if state.event(id).is_none() { return Err(ApiError::NotFound); }
    Ok(Json(Vec::new()))
}

fn valid_transition(from: &EventStatus, to: &EventStatus) -> bool {
    matches!((from, to),
        (EventStatus::Unreviewed, EventStatus::Confirmed | EventStatus::Ignored | EventStatus::Processing) |
        (EventStatus::Processing, EventStatus::Resolved | EventStatus::Ignored) |
        (EventStatus::Confirmed, EventStatus::Ignored | EventStatus::Resolved | EventStatus::Closed) |
        (EventStatus::Ignored, EventStatus::Closed) |
        (EventStatus::Resolved, EventStatus::Closed))
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs().to_string()).unwrap_or_else(|_| "0".into())
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
        let current = database.get_event(id).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::NotFound)?;
        if !valid_transition(&current.status, &status) { return Err(ApiError::Conflict); }
        let mut updated = current.clone(); updated.status = status.clone(); updated.reviewed_at = Some(chrono_like_now());
        let review = EventReview { id: Uuid::new_v4(), event_id: id, old_status: current.status, new_status: status, reviewer: None, note: None, disposition: None, created_at: chrono_like_now() };
        database.save_event(&updated).await.map_err(|_| ApiError::Internal)?;
        database.save_review(&review).await.map_err(|_| ApiError::Internal)?;
        state.events.write().expect("events lock poisoned").insert(updated.id, updated.clone());
        return Ok(Json(updated));
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
