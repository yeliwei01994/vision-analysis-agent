from io import BytesIO

from fastapi.testclient import TestClient
from PIL import Image

from app.main import app, set_detector


class FakeDetector:
    version = "yolov8n-test"

    def predict(self, image_bytes: bytes, timestamp_ms: int):
        return {
            "model_version": self.version,
            "timestamp_ms": timestamp_ms,
            "detections": [
                {
                    "class_name": "person",
                    "confidence": 0.95,
                    "bbox": [1.0, 2.0, 30.0, 40.0],
                    "track_id": None,
                }
            ],
        }


client = TestClient(app)


def test_health_reports_model_service():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["service"] == "yolo-service"


def test_frame_inference_returns_detection_contract():
    image = Image.new("RGB", (32, 32), color="white")
    buffer = BytesIO()
    image.save(buffer, format="JPEG")
    set_detector(FakeDetector())

    response = client.post(
        "/v1/infer/frame",
        files={"file": ("frame.jpg", buffer.getvalue(), "image/jpeg")},
        data={"timestamp_ms": "1200"},
    )

    assert response.status_code == 200
    assert response.json() == {
        "model_version": "yolov8n-test",
        "timestamp_ms": 1200,
        "detections": [
            {
                "class_name": "person",
                "confidence": 0.95,
                "bbox": [1.0, 2.0, 30.0, 40.0],
                "track_id": None,
            }
        ],
    }


def test_batch_inference_returns_one_result_per_frame():
    image = Image.new("RGB", (32, 32), color="white")
    first = BytesIO()
    second = BytesIO()
    image.save(first, format="JPEG")
    image.save(second, format="JPEG")

    class BatchFakeDetector(FakeDetector):
        def predict_batch(self, image_bytes, timestamps_ms):
            return [
                self.predict(image_bytes[0], timestamps_ms[0]),
                self.predict(image_bytes[1], timestamps_ms[1]),
            ]

    set_detector(BatchFakeDetector())
    response = client.post(
        "/v1/infer/batch",
        files=[
            ("files", ("frame-0001.jpg", first.getvalue(), "image/jpeg")),
            ("files", ("frame-0002.jpg", second.getvalue(), "image/jpeg")),
        ],
        data={"metadata": '[{"frame_id":"frame-0001","timestamp_ms":0},{"frame_id":"frame-0002","timestamp_ms":200}]'},
    )

    assert response.status_code == 200
    assert [item["frame_id"] for item in response.json()["items"]] == ["frame-0001", "frame-0002"]
    assert [item["timestamp_ms"] for item in response.json()["items"]] == [0, 200]
