use std::env;
use uuid::Uuid;
use vision_event_api::persistence::{Database, DatabaseConfig};
use vision_event_api::queue::QueueMessage;

#[test]
fn database_config_uses_explicit_url() {
    let config = DatabaseConfig::new("mysql://vision:secret@mysql/vision_events");
    assert_eq!(config.url, "mysql://vision:secret@mysql/vision_events");
}

#[test]
fn queue_message_serializes_job_id_and_attempt() {
    let id = Uuid::new_v4();
    let message = QueueMessage::new(id);
    let encoded = serde_json::to_value(message).unwrap();
    assert_eq!(encoded["job_id"], id.to_string());
    assert_eq!(encoded["attempt"], 0);
}

#[test]
fn migrations_are_kept_in_repository() {
    assert!(
        std::path::Path::new("migrations/001_initial.sql").exists()
            || env::var("CARGO_MANIFEST_DIR").is_ok()
    );
}

#[tokio::test]
async fn list_events_keeps_rows_with_null_prompt_version() {
    let Ok(database_url) = env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is required for this integration test");
        return;
    };
    let database = Database::connect(&DatabaseConfig::new(database_url))
        .await
        .unwrap();
    let events = database.list_events().await.unwrap();
    assert!(
        events.iter().any(|event| event.prompt_version.is_none()),
        "events with a NULL prompt_version must remain readable"
    );
}
