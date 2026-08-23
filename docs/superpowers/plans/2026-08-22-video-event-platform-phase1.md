# Video Event Platform Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 建立可运行的视频事件检索平台第一阶段骨架，使前端、Rust API、事件领域模型、Mock 推理流程和 Docker 部署契约先完整跑通。

**Architecture:** Rust Axum API 提供稳定的 `/api/v1` 契约，领域模型与检测器、大模型分析器通过 trait 解耦。React 前端先使用真实 API 和可降级的 Mock 数据展示任务、事件和详情；Redis/MySQL/MinIO 先通过 Compose 预留，第一阶段 API 使用内存仓储以降低启动门槛。

**Tech Stack:** Rust, Axum, Tokio, Serde, React, Vite, TypeScript, TanStack Query, Docker Compose, Nginx.

**Spec:** `2026-08-21-video-event-retrieval-development.md`

## Global Constraints

- 原图片分析接口保持兼容方向，视频能力新增 `/api/v1`，不删除已有能力。
- 事件类型、规则、Prompt 和模型必须使用可扩展字符串/版本字段，不能把具体检测类别写死在页面或 API 路由中。
- 第一阶段不接真实 YOLO、视频流和真实大模型；使用明确的 Mock Adapter 验证契约。
- 二进制视频、图片和证据不存入 MySQL 或 Redis。
- 任何异步任务和事件生成接口必须具备可观察的状态字段。
- 新增代码必须先有失败测试，再写最小实现。

### Task 1: Repository baseline and Rust domain contract

**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/src/lib.rs`
- Create: `backend/src/domain.rs`
- Create: `backend/tests/domain_contract.rs`

**Interfaces:**
- Produces `VideoJob`, `JobStatus`, `Event`, `EventStatus`, `Detection`, `Evidence` and `AnalysisResult` serializable types.

- [ ] Write a failing serialization test for a video job and event.
- [ ] Run `cargo test` and verify it fails because the domain types do not exist.
- [ ] Implement the domain structs and enums with Serde derives.
- [ ] Run `cargo test` and verify the contract test passes.

### Task 2: Rust API and Mock application flow

**Files:**
- Create: `backend/src/main.rs`
- Create: `backend/src/api.rs`
- Create: `backend/src/application.rs`
- Create: `backend/src/adapters.rs`
- Create: `backend/tests/api_contract.rs`

**Interfaces:**
- `GET /health`
- `POST /api/v1/videos`
- `GET /api/v1/jobs/{id}`
- `GET /api/v1/events`
- `GET /api/v1/events/{id}`
- `POST /api/v1/events/search`

- [ ] Write failing API tests for health, creating a job, listing events and retrieving an event.
- [ ] Run the tests and verify the routes are missing.
- [ ] Implement an in-memory repository and Mock Analyzer/Detector adapters.
- [ ] Add Axum routes and JSON error responses.
- [ ] Run API tests and verify they pass.

### Task 3: React event workspace

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/tsconfig.json`
- Create: `frontend/vite.config.ts`
- Create: `frontend/index.html`
- Create: `frontend/src/main.tsx`
- Create: `frontend/src/App.tsx`
- Create: `frontend/src/api/client.ts`
- Create: `frontend/src/types/events.ts`
- Create: `frontend/src/styles.css`
- Create: `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes the Rust health, job and event endpoints.
- Produces an upload panel, job status, filterable event list and event detail panel.

- [ ] Write a failing component test for the empty event state and event rendering.
- [ ] Run the frontend test and verify the component is missing.
- [ ] Implement the minimal Vite React application with API client and accessible loading/error states.
- [ ] Run frontend tests and build.

### Task 4: Docker, Nginx and development configuration

**Files:**
- Create: `docker-compose.yml`
- Create: `backend/Dockerfile`
- Create: `frontend/Dockerfile`
- Create: `frontend/nginx.conf`
- Create: `.env.example`
- Create: `README.md`

- [ ] Add API, frontend, MySQL and Redis services with health checks and volumes.
- [ ] Configure Nginx SPA fallback, `/api/` proxy and `/media/` placeholder path.
- [ ] Document local development and test commands.
- [ ] Validate Compose configuration and run Rust/frontend checks.

### Task 5: Verification and handoff

- [ ] Run Rust unit and API tests.
- [ ] Run frontend tests and production build.
- [ ] Validate Docker Compose config.
- [ ] Review the implementation against the development document and record known limits.

