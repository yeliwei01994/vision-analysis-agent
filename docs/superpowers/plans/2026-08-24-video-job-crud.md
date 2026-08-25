# Video Job CRUD Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans (recommended) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为视频任务页面增加 MySQL 持久化的编辑、软删除和查询过滤能力。

**Architecture:** 在现有 Axum API、`Database` 持久化层和 React `JobsPage` 上做最小增量改动。删除通过 `deleted_at` 软删除任务并删除事件，物理文件保留；处理中任务由 API 返回 409。

**Tech Stack:** Rust 2021、Axum 0.7、SQLx MySQL、React、TypeScript、Vitest、Docker Compose。

**Spec:** `docs/superpowers/specs/2026-08-24-video-job-crud-design.md`

## Global Constraints

- 仅允许编辑任务显示文件名。
- 状态、进度、时长、物理路径和任务 ID 由系统控制。
- 删除采用软删除：任务写入 `deleted_at`，关联事件删除，物理视频文件保留。
- `queued` 或 `processing` 状态的任务删除返回 HTTP 409。

---

### Task 1: Database soft-delete and job mutation methods

**Files:**
- Create: `db/migrations/002_video_job_soft_delete.sql`
- Modify: `backend/src/persistence.rs`
- Test: `backend/tests/persistence_contract.rs`

**Interfaces:**
- Produces `Database::update_job_filename(Uuid, &str)`, `Database::soft_delete_job(Uuid)`, and filtered `get_job/list_jobs/list_events` behavior.

- [ ] Write failing persistence tests for filtered deleted jobs, filename update, and event removal transaction.
- [ ] Run `cargo test --test persistence_contract` and confirm failure because methods/schema are absent.
- [ ] Add `deleted_at DATETIME NULL` and index migration.
- [ ] Implement SQLx update and transactional soft-delete; reject processing states in the database method with a typed result.
- [ ] Exclude deleted jobs and events joined to deleted jobs from reads.
- [ ] Run the focused persistence test and confirm it passes.

### Task 2: Axum API endpoints and error mapping

**Files:**
- Modify: `backend/src/api.rs`
- Modify: `backend/tests/api_contract.rs`

**Interfaces:**
- Adds `PUT /api/v1/jobs/:id` with `{filename: String}` and `DELETE /api/v1/jobs/:id` with 204/400/404/409 responses.

- [ ] Add failing API contract tests for valid rename, blank/oversized names, missing ID, and processing deletion.
- [ ] Run the focused tests and verify expected failures.
- [ ] Add routes, request DTO, handlers, validation, and `ApiError::Conflict`.
- [ ] Ensure in-memory state and database state are updated consistently after a successful mutation.
- [ ] Run all Rust tests and confirm the API contract passes.

### Task 3: React API client and task page controls

**Files:**
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/features/WorkspacePages.tsx`
- Modify: `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx` or a new task-page test file

**Interfaces:**
- Adds `api.updateJob(id, filename)` and `api.deleteJob(id)`.
- `JobsPage` accepts the existing `onRefresh` callback and invokes the new APIs from edit/delete controls.

- [ ] Add failing UI tests for opening edit control, submitting a new filename, confirming deletion, and showing API errors.
- [ ] Run the focused Vitest test and verify it fails for missing controls/client methods.
- [ ] Implement accessible edit dialog/form, delete confirmation, loading states, and inline error message.
- [ ] Refresh the list only after successful mutation; preserve rows after failures.
- [ ] Run frontend tests and build.

### Task 4: Full verification

**Files:**
- Modify: `README.md` API list if needed

- [ ] Run `cargo test` from `backend`.
- [ ] Run `npm test -- --run` and `npm run build` from `frontend`.
- [ ] Run `docker compose config` from the project root.
- [ ] Inspect `git diff` and verify no generated build artifacts are included.
