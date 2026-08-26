use std::path::PathBuf;
use vision_event_api::yolo::{
    batch_size_from, concurrency_from, YoloBatchItem, YoloBatchResponse, YoloDetection,
    YoloResponse,
};

#[test]
fn yolo_response_maps_to_rule_engine_frame_detections() {
    let response = YoloResponse {
        model_version: "yolov8n".into(),
        timestamp_ms: 1_200,
        detections: vec![YoloDetection {
            class_name: "person".into(),
            confidence: 0.94,
            bbox: [10.0, 20.0, 80.0, 160.0],
            track_id: None,
        }],
    };

    let frames = response.into_frame_detections();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].timestamp_ms, 1_200);
    assert_eq!(frames[0].detection.class_name, "person");
    assert_eq!(frames[0].detection.confidence, 0.94);
}

#[test]
fn yolo_batch_response_keeps_frame_ids_and_detection_timestamps() {
    let response = YoloBatchResponse {
        model_version: "yolov8n".into(),
        items: vec![YoloBatchItem {
            frame_id: "frame-0002.jpg".into(),
            timestamp_ms: 400,
            detections: vec![YoloDetection {
                class_name: "person".into(),
                confidence: 0.91,
                bbox: [0.1, 0.2, 0.8, 0.9],
                track_id: None,
            }],
        }],
    };

    let frames = response
        .into_frame_results(&[PathBuf::from("frames/frame-0002.jpg")])
        .expect("batch frame ids should map");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, PathBuf::from("frames/frame-0002.jpg"));
    assert_eq!(frames[0].1.timestamp_ms, 400);
}

#[test]
fn yolo_batch_and_concurrency_settings_are_bounded() {
    assert_eq!(batch_size_from(None), 4);
    assert_eq!(batch_size_from(Some("32")), 16);
    assert_eq!(batch_size_from(Some("0")), 1);
    assert_eq!(concurrency_from(None), 2);
    assert_eq!(concurrency_from(Some("9")), 4);
    assert_eq!(concurrency_from(Some("invalid")), 2);
}
