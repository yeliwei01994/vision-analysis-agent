# Zone Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add persisted polygon-based intrusion, dwell, and occupancy-limit rules with a dual-background editor.

**Architecture:** Rules gain geometry, threshold, and enabled fields and are stored in MySQL. A pure geometry evaluator produces `RuleEvent` candidates from in-zone detections; the existing worker merge/evidence pipeline persists them. React serializes rectangles as polygons and uses either a blank 16:9 canvas or a selected evidence-frame image as its editor background.

**Tech Stack:** Rust, SQLx/MySQL migrations, Axum, React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-08-25-zone-rules-design.md`

## Global Constraints

- Coordinates are inclusive normalized values from `0.0` through `1.0`.
- Initial rule types are `person_enter_zone`, `person_stay`, and `person_count_limit`.
- Invalid rules return HTTP 400; malformed stored rules are skipped by the Worker without failing the job.
- Existing `person_stay` records remain readable without geometry.
- Do not stage unrelated worktree files.

---

### Task 1: Define Geometry and Rule Contracts

**Files:**
- Modify: `backend/src/rules.rs`, `backend/tests/rule_engine.rs`
- Modify: `frontend/src/types/events.ts`

**Interfaces:**
- Produces `Geometry { kind: Polygon, points: Vec<[f32; 2]> }` and extended `EventRule { geometry: Option<Geometry>, threshold: Option<u32>, enabled: bool }`.
- Produces `point_in_polygon(point: [f32; 2], polygon: &[[f32; 2]]) -> bool` and `RuleEngine::evaluate` candidates.

- [ ] Write failing Rust tests for inside/outside/boundary points, disabled rules, intrusion, dwell, and count greater than threshold.
- [ ] Run `cargo test --manifest-path backend/Cargo.toml --test rule_engine` and confirm failure from missing geometry contract.
- [ ] Implement normalized polygon validation and bottom-center detection placement; preserve the legacy no-geometry `person_stay` behaviour.
- [ ] Run the same tests and confirm pass.
- [ ] Commit `backend/src/rules.rs backend/tests/rule_engine.rs frontend/src/types/events.ts` with `feat: add polygon rule contracts`.

### Task 2: Persist and Validate Rules

**Files:**
- Create: `db/migrations/003_event_rule_geometry.sql`
- Modify: `backend/src/persistence.rs`, `backend/src/application.rs`, `backend/src/api.rs`, `backend/tests/api_contract.rs`

**Interfaces:**
- Adds nullable `geometry_json`, nullable `threshold`, and non-null `enabled` columns to `event_rules`.
- `Database::list_rules() -> Result<Vec<EventRule>, sqlx::Error>` and `Database::save_rule(&EventRule)` are used by API startup/update.

- [ ] Write failing API contract tests for a geometry/threshold/enabled round trip and invalid geometry returning 400.
- [ ] Run `cargo test --manifest-path backend/Cargo.toml --test api_contract event_rules` and confirm failure.
- [ ] Add migration, JSON serialization, request validation, database-backed list/update, and fresh-process rule loading.
- [ ] Run the contract tests and confirm pass.
- [ ] Commit the migration, backend files, and test with `feat: persist zone rule configuration`.

### Task 3: Evaluate All Enabled Rules in the Worker

**Files:**
- Modify: `backend/src/worker.rs`, `backend/tests/worker_contract.rs`

**Interfaces:**
- Consumes `state.event_rules()` and emits merged events for every enabled valid rule.
- Preserves geometry/parameter snapshot alongside each event’s evidence metadata.

- [ ] Write failing test that installs intrusion and count-limit rules, passes synthetic detections, and asserts two correct candidates/events.
- [ ] Run `cargo test --manifest-path backend/Cargo.toml --test worker_contract zone` and confirm failure.
- [ ] Replace the hard-coded `person_stay` lookup with enabled-rule iteration; log and skip malformed geometries; attach snapshot before evidence persistence.
- [ ] Run worker contract tests and confirm pass.
- [ ] Commit with `feat: evaluate persisted zone rules`.

### Task 4: Build the Dual-Background Rule Editor

**Files:**
- Modify: `frontend/src/features/WorkspacePages.tsx`, `frontend/src/api/client.ts`, `frontend/src/App.test.tsx`, `frontend/src/styles.css`

**Interfaces:**
- `api.updateRule` sends geometry, threshold, and enabled fields.
- RulesPage opens an editor with blank/video-frame backgrounds, rectangle/polygon drawing, reset, validation, and save.

- [ ] Write failing Vitest tests for blank default, evidence-background selection, invalid two-point polygon, and outgoing save payload.
- [ ] Run `npm test -- --run --pool=threads --maxWorkers=1 src/App.test.tsx` and confirm failure.
- [ ] Implement the 16:9 canvas, normalized pointer conversion, polygon node editing, rectangle conversion, background fallback, controls, and error display.
- [ ] Run frontend tests and `npm run build`; confirm pass.
- [ ] Commit with `feat: add zone rule editor`.

### Task 5: Verify the Phase

- [ ] Run `cargo test --manifest-path backend/Cargo.toml` with an isolated `CARGO_TARGET_DIR` if the local API locks the default target.
- [ ] Run `npm test -- --run --pool=threads --maxWorkers=1` and `npm run build`.
- [ ] Manually create each initial rule using both backgrounds, restart API/Worker, upload one video, and verify only in-zone detections create events.
- [ ] Commit checklist state with `docs: verify zone rules phase`.
