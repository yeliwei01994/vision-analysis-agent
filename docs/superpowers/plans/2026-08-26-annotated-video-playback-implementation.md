# YOLO Annotated Video Playback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在视频任务处理完成后自动生成完整 YOLO 检测回放，并在事件详情页支持原视频与检测回放切换。

**Architecture:** worker 复用现有 YOLO 抽帧结果，将每一帧标注后用 ffmpeg 编码为 MP4，回放文件保存到 `media/annotated`。`VideoJob` 持久化回放 URL、状态和错误信息；回放失败不影响事件生成和任务完成。前端通过任务关联获取回放信息，在详情页提供播放器和状态降级。

**Tech Stack:** Rust 2021、Axum、SQLx/MySQL、Tokio、image crate、ffmpeg/ffprobe、React、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-26-annotated-video-playback-design.md`

## Global Constraints

- 第一版只支持本地测试环境，不引入 Docker、实时编码或 LLM。
- 原视频、事件流和证据图片不能依赖回放生成成功。
- 媒体 URL 只能是相对 URL，媒体路由必须拒绝目录穿越。
- 旧任务没有回放字段时，前端必须继续正常显示事件详情。
- 处理完成后回放状态为 `ready` 或 `failed`，并保留可读失败原因。

### Task 1: Extend video job playback contract

**Files:**
- Create: `db/migrations/005_annotated_video.sql`
- Modify: `backend/src/domain.rs`
- Modify: `backend/src/persistence.rs`
- Modify: `backend/src/api.rs`
- Modify: `frontend/src/types/events.ts`
- Modify: `frontend/src/api/client.ts`
- Test: `backend/tests/persistence_contract.rs`

**Interfaces:**
- `VideoJob` produces `annotated_video_url: Option<String>`, `annotated_video_status: Option<String>`, and `annotated_video_error: Option<String>`.
- Database methods `save_job` and `get_job` round-trip the three fields.
- `GET /media/...` serves both JPEG evidence and MP4 playback with the correct content type.

- [ ] **Step 1: Write the failing persistence test**

Add an integration test that creates a `VideoJob`, sets playback fields, saves it, reloads it, and asserts URL/status/error survive the round trip. Use the existing `DATABASE_URL` skip convention used by `persistence_contract.rs`.

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --test persistence_contract annotated_video -- --nocapture`

Expected: compile or assertion failure because the job model and database schema do not yet have playback fields.

- [ ] **Step 3: Implement the migration and model contract**

Add nullable columns to `video_jobs`, update `VideoJob` with serde-defaulted optional fields, and update SQL `INSERT`, `SELECT`, and row mapping. Extend the frontend type and add `api.getJob` handling if the current client lacks the typed fields.

- [ ] **Step 4: Update media content type selection**

Return `image/jpeg` for `.jpg`/`.jpeg`, `video/mp4` for `.mp4`, and `application/octet-stream` otherwise. Keep `safe_media_path` unchanged and continue rejecting absolute paths and parent components.

- [ ] **Step 5: Run the focused test and commit**

Run: `cargo test --manifest-path backend/Cargo.toml --test persistence_contract annotated_video -- --nocapture`

Expected: PASS. Commit: `feat: add annotated video job contract`.

### Task 2: Build annotated frames and encode a complete playback

**Files:**
- Modify: `backend/src/storage.rs`
- Modify: `backend/src/video.rs`
- Modify: `backend/src/worker.rs`
- Test: `backend/tests/worker_contract.rs`
- Test: `backend/tests/video_frames.rs`

**Interfaces:**
- `MediaStorage::save_annotated_video(job_id, frames, duration_ms)` returns `Result<String, String>` containing a relative `/media/annotated/{job_id}.mp4` URL.
- `video::encode_frames(input_dir, output_path, duration_ms)` invokes ffmpeg at 2 FPS, preserves all sampled frames, and returns a descriptive error on missing encoder or failed encoding.
- `worker::process_job` persists playback status independently of event persistence.

- [ ] **Step 1: Write failing encoder/storage tests**

Add tests for deterministic annotated output path, `select_evidence_frames`-independent full-frame input, and an encoder command contract. The encoder test should create a temporary JPEG sequence and skip only when ffmpeg is unavailable; when available it must assert an MP4 is produced and non-empty.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract annotated -- --nocapture` and `cargo test --manifest-path backend/Cargo.toml --test video_frames annotated -- --nocapture`.

Expected: FAIL because full-video annotation and encoding functions do not exist.

- [ ] **Step 3: Extract reusable annotation logic**

Make the existing image annotation routine reusable for arbitrary frame output. Draw every detection using normalized coordinates, class label, confidence, and a stable color. Ensure empty-detection frames are copied/encoded instead of dropped.

- [ ] **Step 4: Implement ffmpeg encoding**

Write annotated JPEGs under a job-specific temporary directory, call `ffmpeg -hide_banner -loglevel error -framerate 2 -i frame-%010d.jpg -c:v libx264 -pix_fmt yuv420p -movflags +faststart -t <duration> output.mp4`, and remove temporary files after success or failure. Reject empty frame sets with a clear error.

- [ ] **Step 5: Integrate worker lifecycle**

Set job playback status to `pending` before encoding, persist it, then set `ready` and URL on success or `failed` and error on failure. Do not return `false` solely because playback encoding fails. Use the full `frames` collection and the task duration, not the limited evidence-frame subset.

- [ ] **Step 6: Run focused tests and commit**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract --test video_frames -- --nocapture`.

Expected: PASS. Commit: `feat: generate annotated YOLO playback videos`.

### Task 3: Connect playback data to event details

**Files:**
- Modify: `backend/src/api.rs`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- The selected event's `job_id` resolves to a `VideoJob` from the existing jobs list or `api.getJob`.
- The detail panel renders a video source only when the selected job has `annotated_video_status === 'ready'` and a URL.
- Playback states are visible as `生成中`, `可播放`, or `生成失败：原因`.

- [ ] **Step 1: Write failing UI tests**

Add tests for a ready playback showing the video element and toggle buttons, a pending/failed playback showing status text without a broken player, and preserving the evidence timeline when no playback exists.

- [ ] **Step 2: Run frontend tests and verify failure**

Run: `npm test -- --run src/App.test.tsx --pool=threads --maxWorkers=1`.

Expected: FAIL because no playback controls or video element exist.

- [ ] **Step 3: Add selected job playback resolution**

Use the existing `jobs` state to find `selected.job_id`; if absent, leave the evidence UI intact. Avoid making the event detail dependent on an additional request for the first version.

- [ ] **Step 4: Add player and source toggle**

Render `原始视频` when `source_uri` exists and `YOLO 检测回放` when playback is ready. Add a `<video controls preload="metadata">` player, preserve the evidence image/timeline below it, and show a non-empty fallback for pending/failed/no-video states.

- [ ] **Step 5: Add status and responsive styles**

Style the playback panel consistently with the existing dark detail layout, keep controls keyboard-accessible, and prevent a missing source from creating a blank or broken media box.

- [ ] **Step 6: Run frontend tests/build and commit**

Run: `npm test -- --run --pool=threads --maxWorkers=1` and `npm run build`.

Expected: all tests pass and build succeeds. Commit: `feat: add annotated playback to event details`.

### Task 4: Add lifecycle cleanup and final verification

**Files:**
- Modify: `backend/src/storage.rs`
- Modify: `backend/src/api.rs`
- Modify: `backend/src/persistence.rs`
- Modify: `backend/tests/api_contract.rs`
- Modify: `backend/tests/persistence_contract.rs`

- [ ] **Step 1: Write failing cleanup and API tests**

Cover serving an MP4 through `/media`, rejecting unsafe playback paths, and deleting a job's annotated output when the job is deleted. Keep the original upload untouched.

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract --test persistence_contract -- --nocapture`.

Expected: FAIL for the new playback cases before cleanup support is implemented.

- [ ] **Step 3: Implement cleanup and regression handling**

Delete only the exact `media/annotated/{job_id}.mp4` path during job deletion, tolerate missing files, and keep event deletion behavior unchanged. Ensure old rows with null playback fields deserialize successfully.

- [ ] **Step 4: Run the complete verification suite**

Run: `npm test -- --run --pool=threads --maxWorkers=1`, `npm run build`, and `$env:CARGO_NET_OFFLINE='true'; cargo test --manifest-path backend/Cargo.toml`.

Expected: all frontend and Rust tests pass, including media safety, worker, persistence, and API contracts.

- [ ] **Step 5: Commit the final cleanup**

Commit: `test: verify annotated playback lifecycle`.
