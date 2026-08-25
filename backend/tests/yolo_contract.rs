use vision_event_api::yolo::{YoloDetection, YoloResponse};

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
