import os

from fastapi import FastAPI, File, Form, UploadFile

from .detector import YoloDetector
from .schemas import InferenceResponse

app = FastAPI(title="YOLOv8n Inference Service", version="0.1.0")
_detector = YoloDetector()


def set_detector(detector: object) -> None:
    global _detector
    _detector = detector


@app.get("/health")
async def health() -> dict[str, str]:
    return {
        "status": "ok",
        "service": "yolo-service",
        "model": os.getenv("YOLO_MODEL", "yolov8n.pt"),
        "device": os.getenv("YOLO_DEVICE", "cpu"),
    }


@app.post("/v1/infer/frame", response_model=InferenceResponse)
async def infer_frame(file: UploadFile = File(...), timestamp_ms: int = Form(0)) -> InferenceResponse:
    image_bytes = await file.read()
    return _detector.predict(image_bytes, timestamp_ms)
