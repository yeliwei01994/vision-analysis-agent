import io
import os
from typing import Any

from PIL import Image

from .schemas import InferenceResponse


class YoloDetector:
    def __init__(self, model_name: str | None = None, device: str | None = None, confidence: float | None = None):
        self.model_name = model_name or os.getenv("YOLO_MODEL", "yolov8n.pt")
        self.device = device or os.getenv("YOLO_DEVICE", "cpu")
        self.confidence = confidence if confidence is not None else float(os.getenv("YOLO_CONFIDENCE", "0.25"))
        self._model: Any = None

    @property
    def version(self) -> str:
        return self.model_name.removesuffix(".pt")

    def _load(self) -> Any:
        if self._model is None:
            from ultralytics import YOLO

            self._model = YOLO(self.model_name)
        return self._model

    def predict(self, image_bytes: bytes, timestamp_ms: int) -> InferenceResponse:
        image = Image.open(io.BytesIO(image_bytes)).convert("RGB")
        results = self._load().predict(
            source=image,
            conf=self.confidence,
            device=self.device,
            verbose=False,
        )
        detections: list[dict[str, Any]] = []
        width, height = image.size
        for result in results:
            names = result.names
            boxes = result.boxes
            for index in range(len(boxes)):
                class_id = int(boxes.cls[index].item())
                detections.append(
                    {
                        "class_name": names[class_id],
                        "confidence": float(boxes.conf[index].item()),
                        "bbox": [
                            float(boxes.xyxy[index][0].item()) / width,
                            float(boxes.xyxy[index][1].item()) / height,
                            float(boxes.xyxy[index][2].item()) / width,
                            float(boxes.xyxy[index][3].item()) / height,
                        ],
                        "track_id": None,
                    }
                )
        print(
            f"YOLO inference: timestamp_ms={timestamp_ms}, "
            f"detections={len(detections)}, "
            f"classes={[item['class_name'] for item in detections]}",
            flush=True,
        )
        return InferenceResponse(model_version=self.version, timestamp_ms=timestamp_ms, detections=detections)
