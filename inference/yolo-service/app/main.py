import asyncio
import json
import os

from fastapi import FastAPI, File, Form, HTTPException, UploadFile

from .detector import YoloDetector
from .schemas import BatchFrameMetadata, BatchInferenceResponse, InferenceResponse

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


@app.post("/v1/infer/batch", response_model=BatchInferenceResponse)
async def infer_batch(
    files: list[UploadFile] = File(...),
    metadata: str = Form(...),
) -> BatchInferenceResponse:
    try:
        frame_metadata = [BatchFrameMetadata.model_validate(item) for item in json.loads(metadata)]
    except (json.JSONDecodeError, TypeError, ValueError) as exc:
        raise HTTPException(status_code=400, detail="metadata must be a JSON array of frame metadata") from exc

    if not files or len(files) != len(frame_metadata):
        raise HTTPException(status_code=400, detail="files and metadata must have the same non-zero length")

    image_bytes = [await file.read() for file in files]
    timestamps_ms = [item.timestamp_ms for item in frame_metadata]
    raw_predictions = await asyncio.to_thread(_detector.predict_batch, image_bytes, timestamps_ms)
    predictions = [InferenceResponse.model_validate(prediction) for prediction in raw_predictions]
    if len(predictions) != len(frame_metadata):
        raise HTTPException(status_code=502, detail="detector returned an unexpected result count")

    return {
        "model_version": predictions[0].model_version,
        "items": [
            {
                "frame_id": item.frame_id,
                "timestamp_ms": prediction.timestamp_ms,
                "detections": prediction.detections,
            }
            for item, prediction in zip(frame_metadata, predictions)
        ],
    }
