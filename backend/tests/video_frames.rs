use vision_event_api::video::frame_timestamp_ms;

#[test]
fn parses_timestamp_from_ffmpeg_frame_name() {
    assert_eq!(frame_timestamp_ms("frame-0001200.jpg"), Some(1200));
    assert_eq!(frame_timestamp_ms("not-a-frame.jpg"), None);
}
