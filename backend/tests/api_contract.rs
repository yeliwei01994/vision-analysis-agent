use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use tower::ServiceExt;
use vision_event_api::{api, application::AppState};

fn app() -> Router {
    api::router(AppState::default())
}

#[tokio::test]
async fn health_returns_ok() {
    let response = app()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_video_and_list_seed_event() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"demo.mp4","duration_ms":12000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = service
        .oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let events: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(events.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn upload_then_process_video_returns_completed_job() {
    let service = app();
    let body = b"--demo\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\nfake-video\r\n--demo--\r\n";
    let response = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos/upload")
                .header("content-type", "multipart/form-data; boundary=demo")
                .body(Body::from(body.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = payload["id"].as_str().unwrap();
    assert!(payload["source_uri"]
        .as_str()
        .unwrap()
        .ends_with("clip.mp4"));

    let response = service
        .oneshot(
            Request::post(format!("/api/v1/videos/{id}/process"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let processed: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(processed["status"], "completed");
    assert_eq!(processed["progress"], 100);
}

#[tokio::test]
async fn event_rules_can_be_listed_and_updated() {
    let service = app();
    let response = service
        .clone()
        .oneshot(
            Request::get("/api/v1/event-rules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let defaults: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(defaults
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["event_type"] == "person_stay"));

    let response = service
        .oneshot(
            Request::put("/api/v1/event-rules/person_stay")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"class_name":"person","min_confidence":0.75,"min_duration_ms":2000}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(updated["min_duration_ms"], 2000);
}

#[tokio::test]
async fn event_can_be_confirmed_and_ignored() {
    let service = app();
    let create = service
        .clone()
        .oneshot(
            Request::post("/api/v1/videos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"review.mp4","duration_ms":12000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    let events_response = service
        .clone()
        .oneshot(Request::get("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let events: serde_json::Value = serde_json::from_slice(
        &events_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let id = events[0]["id"].as_str().unwrap();

    let confirmed = service
        .clone()
        .oneshot(
            Request::post(format!("/api/v1/events/{id}/confirm"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    let confirmed_body: serde_json::Value = serde_json::from_slice(
        &confirmed
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(confirmed_body["status"], "confirmed");

    let ignored = service
        .oneshot(
            Request::post(format!("/api/v1/events/{id}/ignore"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ignored.status(), StatusCode::OK);
    let ignored_body: serde_json::Value = serde_json::from_slice(
        &ignored
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(ignored_body["status"], "ignored");
}
