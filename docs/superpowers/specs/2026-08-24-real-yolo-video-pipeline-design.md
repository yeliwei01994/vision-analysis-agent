# Real YOLO Video Pipeline Design

## Goal

Upload a video once, automatically process it through Rust Worker frame extraction and the HTTP YOLO service, persist real detections/events, and expose the detector/model version to the frontend.

## Architecture

The upload endpoint saves the media and enqueues the job immediately when a queue is configured. The existing process endpoint remains as an idempotent compatibility entry point. The Worker probes the video, extracts JPEG frames with ffmpeg at a bounded interval, sends each frame to `POST /v1/infer/frame`, converts the response into the existing `FrameDetection` domain type, and evaluates rules over real detections.

`YOLO_URL` defaults to `http://localhost:9000` for local execution and is set to `http://yolo:9000` for Docker services. `YoloDetector` owns the HTTP contract and reports the service's `model_version`; no Python inference code is moved into Rust.

## Data flow

```text
POST /videos/upload
  -> save media + create job + enqueue once
  -> Worker: ffprobe duration
  -> ffmpeg frame extraction
  -> Rust YoloDetector -> POST /v1/infer/frame
  -> real detections + model version
  -> rule engine + MySQL/in-memory event
  -> frontend job polling + event object/version display
```

## Decisions

- Frame extraction uses ffmpeg's `fps` filter and produces timestamped JPEGs in a temporary job directory.
- The HTTP client sends multipart `file` and `timestamp_ms`, with a request timeout and non-2xx error handling.
- Missing YOLO or frame extraction errors fail the job instead of silently producing mock detections.
- `MockDetector` is removed from the Worker production path; tests inject a detector or use a local HTTP test server.
- Upload auto-enqueue is idempotent through a job-level queued/processing guard; `/process` returns the current job when already queued.
- Existing dirty files are preserved and only files required for this feature are changed.

## Testing

- Rust unit tests cover frame timestamp parsing/extraction, YOLO response mapping, and auto-enqueue behavior.
- YOLO Python contract tests remain and validate `model_version` plus detection payload.
- Frontend tests cover rendering real object class names and model version.
- One Compose-backed E2E test uploads the checked-in sample MP4, waits for completion, and asserts a persisted event contains a non-mock detector version and real detection objects.

