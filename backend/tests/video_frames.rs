use vision_event_api::video::{detection_interval_from, frame_timestamp_ms, parse_frame_rate, playback_duration_ms, DETECTION_INTERVAL_MS, REPLAY_FPS};

#[test]
fn parses_timestamp_from_ffmpeg_frame_name() {
    assert_eq!(frame_timestamp_ms("frame-0001200.jpg"), Some(1200));
    assert_eq!(frame_timestamp_ms("not-a-frame.jpg"), None);
}

#[test]
fn replay_uses_smooth_sampling_rate() {
    assert_eq!(DETECTION_INTERVAL_MS, 200);
    assert_eq!(REPLAY_FPS, 10);
}

#[test]
fn detection_interval_accepts_safe_configuration_bounds() {
    assert_eq!(detection_interval_from(None), 200);
    assert_eq!(detection_interval_from(Some("100")), 100);
    assert_eq!(detection_interval_from(Some("50")), 100);
    assert_eq!(detection_interval_from(Some("99999")), 5000);
    assert_eq!(detection_interval_from(Some("invalid")), 200);
}

#[test]
fn parses_common_ffprobe_frame_rates() {
    assert_eq!(parse_frame_rate("30/1"), Some(30.0));
    assert_eq!(parse_frame_rate("30000/1001"), Some(29.97002997002997));
    assert_eq!(parse_frame_rate("0/0"), None);
}

#[test]
fn playback_duration_uses_source_video_not_detection_count() {
    assert_eq!(playback_duration_ms(8_880, 24_200), 8_880);
    assert_eq!(playback_duration_ms(0, 9_000), 9_000);
}
