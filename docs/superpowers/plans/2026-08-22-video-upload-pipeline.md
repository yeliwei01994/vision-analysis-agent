# Video Upload Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将第一阶段的 Mock 视频任务升级为可上传视频、保存媒体文件、查询任务进度并触发处理的第二阶段链路。

**Architecture:** Axum 负责 multipart 上传和任务 API，`MediaStorage` 负责文件系统边界，`VideoProbe` 封装 FFprobe 元信息读取，`MockWorker` 负责可观察的处理状态和 Mock 事件生成。未来替换 YOLO 时只需要替换 Worker 内的 Detector Adapter。

**Tech Stack:** Rust, Axum multipart, Tokio fs/process, FFmpeg/FFprobe, React, TypeScript, Docker Compose.

**Spec:** `2026-08-21-video-event-retrieval-development.md`

## Global Constraints

- 视频二进制保存到媒体目录，不写入 MySQL 或 Redis。
- 上传接口使用 multipart，文件名必须经过安全清洗，不能允许路径穿越。
- 第一阶段 Worker 使用 Mock Detector，不接真实 YOLO 权重。
- API 错误返回结构化 JSON，任务状态可查询。
- FFprobe 不可用时，上传仍能创建任务，元信息读取失败必须可观察。

### Task 1: Storage and upload contract

**Files:** `backend/src/storage.rs`, `backend/src/video.rs`, `backend/tests/upload_contract.rs`, `backend/Cargo.toml`

- [ ] Write failing tests for safe filename handling, upload persistence and missing media directory creation.
- [ ] Run tests and observe missing storage API failure.
- [ ] Implement `MediaStorage::save_upload` and `sanitize_filename`.
- [ ] Run tests and verify they pass.

### Task 2: Upload and process API

**Files:** `backend/src/api.rs`, `backend/src/application.rs`, `backend/src/lib.rs`, `backend/tests/api_contract.rs`

- [ ] Write failing API tests for multipart upload, process trigger and job status.
- [ ] Implement routes `POST /api/v1/videos/upload`, `POST /api/v1/videos/{id}/process`, and `GET /api/v1/jobs/{id}`.
- [ ] Persist job media path and status in the in-memory application repository.
- [ ] Run API tests and verify the complete request flow.

### Task 3: Probe and worker progress

**Files:** `backend/src/video.rs`, `backend/src/worker.rs`, `backend/src/main.rs`, `backend/Dockerfile`

- [ ] Add FFprobe command adapter with a deterministic fallback when the binary is unavailable.
- [ ] Add mock processing that updates progress and creates an event.
- [ ] Add FFmpeg to the API image and media volume configuration.
- [ ] Run Rust tests and validate the container configuration.

### Task 4: React upload workflow

**Files:** `frontend/src/api/client.ts`, `frontend/src/App.tsx`, `frontend/src/App.test.tsx`, `frontend/src/styles.css`

- [ ] Write a failing UI test for selecting a video and showing processing status.
- [ ] Add multipart upload client and task status polling.
- [ ] Replace demo-only import action with a file picker while preserving API fallback.
- [ ] Run frontend tests and production build.

### Task 5: Verification

- [ ] Run `cargo test`.
- [ ] Run frontend tests and build.
- [ ] Run `docker compose config`.
- [ ] Review the implementation against this plan and document known limitations.

