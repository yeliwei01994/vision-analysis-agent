from pydantic import BaseModel, Field


class Detection(BaseModel):
    class_name: str
    confidence: float = Field(ge=0.0, le=1.0)
    bbox: list[float] = Field(min_length=4, max_length=4)
    track_id: int | None = None


class InferenceResponse(BaseModel):
    model_version: str
    timestamp_ms: int = Field(ge=0)
    detections: list[Detection]


class BatchFrameMetadata(BaseModel):
    frame_id: str = Field(min_length=1)
    timestamp_ms: int = Field(ge=0)


class BatchInferenceItem(BaseModel):
    frame_id: str = Field(min_length=1)
    timestamp_ms: int = Field(ge=0)
    detections: list[Detection]


class BatchInferenceResponse(BaseModel):
    model_version: str
    items: list[BatchInferenceItem]
