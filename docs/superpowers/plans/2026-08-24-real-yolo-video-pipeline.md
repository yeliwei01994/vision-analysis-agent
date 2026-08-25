# Real YOLO Video Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace mock video analysis with automatic Rust-to-YOLO frame inference and expose real detections/model metadata end to end.

**Architecture:** API auto-enqueues uploads, Worker extracts frames with ffmpeg, `YoloDetector` calls the existing FastAPI endpoint, and the existing rule/persistence/frontend layers consume the real results.

**Tech Stack:** Rust/Axum/Tokio/Reqwest, ffmpeg, FastAPI/Ultralytics, React/Vite, Docker Compose, MySQL, Redis.

**Spec:** `docs/superpowers/specs/2026-08-24-real-yolo-video-pipeline-design.md`

## Global Constraints

- `YOLO_URL` defaults to `http://localhost:9000` locally and is `http://yolo:9000` in Docker.
- No production Worker path may instantiate `MockDetector`.
- Existing uncommitted user changes must be preserved.
- Every behavior change gets a failing test before implementation.

### Task 1: Rust YOLO HTTP client and frame extraction

**Files:**
- Modify: `backend/Cargo.toml`
- Modify: `backend/src/adapters.rs`
- Modify: `backend/src/video.rs`
- Create: `backend/tests/yolo_client.rs`
- Create: `backend/tests/video_frames.rs`

- [ ] Write failing tests for response mapping, model version propagation, and timestamped frame extraction.
- [ ] Run `cargo test --test yolo_client --test video_frames` and confirm feature-missing failures.
- [ ] Add reqwest multipart client, `YOLO_URL` configuration, and ffmpeg frame extraction with cleanup-safe temporary paths.
- [ ] Run the focused tests and confirm they pass.

### Task 2: Worker real detection pipeline

**Files:**
- Modify: `backend/src/worker.rs`
- Modify: `backend/src/application.rs`
- Modify: `backend/src/domain.rs`
- Create or modify: `backend/tests/worker_yolo.rs`

- [ ] Write a failing Worker test using an injected fake detector and temporary sample frames.
- [ ] Run the focused test and confirm it fails because Worker uses fixed mock timestamps.
- [ ] Inject a detector into `AppState`, extract frames from `source_uri`, call the detector, and persist detector/model version on generated events.
- [ ] Add failed-job status updates for ffmpeg or YOLO errors.
- [ ] Run focused Worker tests and then all Rust tests.

### Task 3: Upload auto-enqueue and Docker/local configuration

**Files:**
- Modify: `backend/src/api.rs`
- Modify: `backend/src/queue.rs`
- Modify: `docker-compose.yml`
- Modify: `.env.example`
- Modify: `backend/Dockerfile`
- Modify: `README.md`

- [ ] Add a failing API contract test proving upload enqueues once and repeated process calls are idempotent.
- [ ] Run the contract test and confirm it fails.
- [ ] Auto-enqueue after upload when a queue exists, preserve `/process`, add `YOLO_URL` to API/Worker Docker environments, and ensure ffmpeg is available in the Worker image.
- [ ] Run Rust tests and `docker compose config`.

### Task 4: Frontend real detection/model display

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/features/WorkspacePages.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/App.test.tsx`

- [ ] Add failing UI assertions for actual class names, bounding-box/object count, and detector/model version.
- [ ] Run the focused Vitest test and confirm failure.
- [ ] Replace hard-coded `PERSON` display with real object summary and render `detector_version`.
- [ ] Remove the extra frontend process call while retaining compatibility in the API.
- [ ] Run frontend tests and production build.

### Task 5: Full end-to-end test

**Files:**
- Create: `scripts/e2e-yolo.ps1`
- Create: `backend/tests/e2e_yolo.rs` or repository E2E test harness
- Modify: `README.md`

- [ ] Write the E2E test for upload, automatic queueing, completion, event retrieval, real object, and non-mock model version.
- [ ] Run it against the Compose stack and confirm the pre-implementation failure if the stack is available.
- [ ] Make the harness wait for service health and job completion without fixed sleeps.
- [ ] Run the complete E2E plus Rust/Python/Frontend checks.

