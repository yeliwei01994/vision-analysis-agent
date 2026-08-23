# Persistence and Queue Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 MySQL、Redis 从容器依赖接入真实任务和事件业务路径，并让 Worker 支持独立进程运行。

**Architecture:** SQLx MySQL 保存视频任务、事件和规则，Redis Streams 只保存可重建的任务消息与进度。API 创建任务后写 MySQL，再把任务 ID 投递 Redis；Worker 消费任务、处理视频并回写 MySQL。Redis 不可用时 API 明确返回降级状态，不伪装成已入队。

**Tech Stack:** Rust, SQLx MySQL, redis-rs, Redis Streams, Tokio, Nginx, Docker Compose.

**Spec:** `2026-08-21-video-event-retrieval-development.md`

## Global Constraints

- MySQL 是任务和事件的最终数据源。
- Redis 不保存视频二进制，也不作为事实数据源。
- 单元测试使用内存 AppState；集成测试通过环境变量启用 MySQL/Redis。
- 数据库密码和 Redis 地址只来自环境变量。
- Worker 失败必须把任务标记为 `failed` 并保留错误信息。

### Task 1: Persistence and queue contracts

**Files:** `backend/src/persistence.rs`, `backend/src/queue.rs`, `backend/tests/persistence_contract.rs`, `backend/Cargo.toml`

- [ ] Write failing tests for database URL configuration, Redis stream message encoding, and migration file presence.
- [ ] Implement SQLx/Redis wrappers with optional runtime configuration.
- [ ] Run offline Rust tests for pure contract behavior.

### Task 2: MySQL schema and repository

**Files:** `backend/migrations/001_initial.sql`, `backend/src/persistence.rs`, `backend/src/application.rs`

- [ ] Create `video_jobs`, `events`, and `event_rules` tables.
- [ ] Persist jobs, events, rules and query events from MySQL.
- [ ] Keep memory fallback for unit tests and unavailable infrastructure.

### Task 3: Redis Streams and Worker mode

**Files:** `backend/src/queue.rs`, `backend/src/worker.rs`, `backend/src/main.rs`, `docker-compose.yml`

- [ ] Add `XADD` task publishing and `XREADGROUP` consumption.
- [ ] Add `WORKER_MODE=1` process mode.
- [ ] API process endpoint publishes a job instead of executing inline when Redis is configured.
- [ ] Worker acknowledges successful messages and records failure status.

### Task 4: API and Nginx integration

**Files:** `backend/src/api.rs`, `frontend/nginx.conf`, `.env.example`, `README.md`

- [ ] Expose infrastructure health status without leaking credentials.
- [ ] Route `/media/` to the shared media directory or object storage proxy.
- [ ] Document Compose startup, migrations and worker logs.

### Task 5: Verification

- [ ] Run Rust unit/API tests and formatting.
- [ ] Run frontend tests/build.
- [ ] Run `docker compose config`.
- [ ] If Docker daemon is available, run a MySQL/Redis integration smoke test.

