# Event Operations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement C1 review operations, C2 filtered event query/export, and C3 association/HTML reporting without breaking current local workflows.

**Architecture:** Extend the existing Rust domain and MySQL persistence model with review metadata, history, query DTOs, and association keys. Keep old endpoints as wrappers, add bounded query/export/report endpoints, and update the existing React event detail/list page with review, filters, pagination, and report controls.

**Tech Stack:** Rust, Axum, SQLx MySQL, serde, React, TypeScript, Vitest, Vite.

**Spec:** `docs/superpowers/specs/2026-08-25-event-operations-design.md`

## Global Constraints

- Local MySQL and in-memory mode must remain supported.
- Existing confirm/ignore endpoints remain compatible.
- Reports must work without LLM configuration.
- Do not change upload, YOLO, region filtering, or evidence URL contracts.

### Task 1: C1 domain and schema

**Files:**
- Create: `db/migrations/004_event_operations.sql`
- Modify: `backend/src/domain.rs`
- Modify: `backend/src/persistence.rs`
- Test: `backend/tests/domain_contract.rs`, `backend/tests/persistence_contract.rs`

- [ ] Add the six event statuses and review fields to `Event`.
- [ ] Add `EventReview`, `ReviewRequest`, and stable serde names.
- [ ] Add migration columns for reviewer, reviewed time, note, disposition, zone key, association key.
- [ ] Add `event_review_history` with indexes on event and created time.
- [ ] Write failing tests for status serialization and transition history mapping.
- [ ] Implement persistence save/load and history methods.
- [ ] Run focused Rust tests and commit.

### Task 2: C1 API and frontend

**Files:**
- Modify: `backend/src/api.rs`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/types/events.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles.css`
- Test: `backend/tests/api_contract.rs`, `frontend/src/App.test.tsx`

- [ ] Add `POST /api/v1/events/:id/review` and `GET /api/v1/events/:id/reviews`.
- [ ] Keep confirm/ignore wrappers and validate legal transitions.
- [ ] Add review dialog fields, timeline rendering, success/error feedback, and duplicate-submit disabling.
- [ ] Write failing API/UI tests first, then implement and run focused tests.
- [ ] Commit C1.

### Task 3: C2 query and export

**Files:**
- Modify: `backend/src/persistence.rs`
- Modify: `backend/src/api.rs`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/types/events.ts`
- Modify: `frontend/src/styles.css`
- Test: `backend/tests/api_contract.rs`, `frontend/src/App.test.tsx`

- [ ] Define query params and a paginated response.
- [ ] Implement parameterized SQL predicates, stable sorting, bounded page size, and total count.
- [ ] Implement UTF-8 CSV using the same query filters.
- [ ] Add indexes in the migration.
- [ ] Add filter controls, pagination, and export button to the event stream.
- [ ] Write failing query/CSV tests, implement, run focused tests, and commit C2.

### Task 4: C3 association and report

**Files:**
- Modify: `backend/src/domain.rs`
- Modify: `backend/src/persistence.rs`
- Modify: `backend/src/api.rs`
- Modify: `backend/src/worker.rs`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/App.tsx`
- Test: `backend/tests/domain_contract.rs`, `backend/tests/api_contract.rs`, `frontend/src/App.test.tsx`

- [ ] Derive association keys from job, zone, rule, and overlapping track IDs.
- [ ] Expose related event IDs in event responses.
- [ ] Render a local HTML report with evidence links, summary, timeline, and review conclusion.
- [ ] Add report and association controls to event detail.
- [ ] Write failing association/report tests, implement, run focused tests, and commit C3.

### Task 5: Full verification

- [ ] Run all Rust tests in an isolated target directory.
- [ ] Run all frontend tests and production build.
- [ ] Run `git diff --check` and inspect migration/API compatibility.
- [ ] Verify the acceptance checklist against the spec.
- [ ] Commit any final corrections and report exact test evidence.
