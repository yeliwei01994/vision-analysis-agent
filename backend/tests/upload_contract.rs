use uuid::Uuid;
use vision_event_api::storage::{sanitize_filename, MediaStorage};

fn temp_media_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("vision-event-test-{}", Uuid::new_v4()))
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

#[tokio::test]
async fn finalizes_a_streamed_upload_without_leaving_a_partial_file() {
    let root = temp_media_dir();
    let storage = MediaStorage::new(root.clone());
    let temporary = storage.create_upload_temp("camera/demo.mp4").await.unwrap();
    tokio::fs::write(&temporary, b"streamed-video").await.unwrap();
    let saved = storage.finalize_upload(temporary.clone(), "camera/demo.mp4").await.unwrap();

    assert_eq!(tokio::fs::read(&saved).await.unwrap(), b"streamed-video");
    assert!(!temporary.exists());
    tokio::fs::remove_dir_all(root).await.unwrap();
}
