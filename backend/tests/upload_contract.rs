use std::time::{SystemTime, UNIX_EPOCH};
use vision_event_api::storage::{sanitize_filename, MediaStorage};

fn temp_media_dir() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("vision-event-test-{nonce}"))
}

#[tokio::test]
async fn sanitizes_filename_without_path_components() {
    assert_eq!(
        sanitize_filename("../../unsafe video.mp4"),
        "unsafe_video.mp4"
    );
}

#[tokio::test]
async fn saves_uploaded_bytes_and_creates_media_directory() {
    let root = temp_media_dir();
    let storage = MediaStorage::new(root.clone());
    let saved = storage
        .save_upload("camera/demo.mp4", b"fake-video")
        .await
        .unwrap();
    assert!(saved.starts_with(&root));
    assert_eq!(tokio::fs::read(&saved).await.unwrap(), b"fake-video");
    tokio::fs::remove_dir_all(root).await.unwrap();
}
