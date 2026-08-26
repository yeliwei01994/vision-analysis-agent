use vision_event_api::video::{frame_timestamp_ms, DETECTION_INTERVAL_MS, REPLAY_FPS};

#[test]
fn parses_timestamp_from_ffmpeg_frame_name() {
    assert_eq!(frame_timestamp_ms("frame-0001200.jpg"), Some(1200));
    assert_eq!(frame_timestamp_ms("not-a-frame.jpg"), None);
}

#[test]
fn replay_uses_smooth_sampling_rate() {
    assert_eq!(DETECTION_INTERVAL_MS, 100);
    assert_eq!(REPLAY_FPS, 10);
}
