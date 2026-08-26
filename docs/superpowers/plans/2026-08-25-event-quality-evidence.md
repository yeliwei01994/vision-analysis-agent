# Event Quality and Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge continuous same-class detections into one event and retain a bounded, reliable set of representative evidence frames.

**Architecture:** The Worker will aggregate `RuleEvent` candidates before persistence, then select a bounded chronological evidence set before `MediaStorage` copies JPEGs. Storage owns event-evidence deletion; API composes database deletion with storage cleanup and media response headers; React handles image failures without hiding the rest of the event detail.

**Tech Stack:** Rust 2021, Tokio, Axum, SQLx/MySQL, React, TypeScript, Vitest, Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-25-event-quality-evidence-design.md`

## Global Constraints

- Apply merge and sampling only to newly processed videos; do not migrate existing event JSON.
- Merge only equal job, rule version, event type, and primary class when their temporal gap is at most 3,000 ms.
- Select first, peak-confidence, last, and 5,000 ms samples; save at most 12 unique frames per event.
- Keep `Evidence.frames`, `frame_urls`, and `thumbnail_url` backward compatible.
- Never derive a filesystem path from an untrusted value; storage must construct event paths from a parsed UUID.
- Do not commit unrelated pre-existing worktree changes.

---

## File Structure

- `backend/src/worker.rs` — pure candidate aggregation and representative-frame selection, then Worker integration.
- `backend/src/storage.rs` — scoped evidence-directory deletion.
- `backend/src/api.rs` — cache headers and API-to-storage deletion composition.
- `backend/tests/worker_contract.rs` — merge and selection contract tests.
- `backend/tests/api_contract.rs` — delete/media HTTP contracts.
- `frontend/src/App.tsx` — image failure state and retry handler.
- `frontend/src/App.test.tsx` — evidence-image failure contract.
- `frontend/src/styles.css` — recovery-state styling.

### Task 1: Merge Rule Candidates in the Worker

**Files:**
- Modify: `backend/src/worker.rs`
- Test: `backend/tests/worker_contract.rs`

**Interfaces:**
- Consumes: `RuleEvent { event_type, start_time_ms, end_time_ms, confidence, objects, frames, rule_version }`.
- Produces: `pub(crate) fn merge_rule_events(candidates: Vec<RuleEvent>, gap_ms: u64) -> Vec<RuleEvent>`.

- [ ] **Step 1: Write failing merge tests**

```rust
#[test]
fn merges_matching_candidates_within_the_gap() {
    let events = merge_rule_events(vec![candidate(0, 500), candidate(1_000, 1_500)], 3_000);
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].start_time_ms, events[0].end_time_ms), (0, 1_500));
}

#[test]
fn keeps_candidates_separated_by_more_than_the_gap() {
    assert_eq!(merge_rule_events(vec![candidate(0, 500), candidate(3_501, 4_000)], 3_000).len(), 2);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract merge`

Expected: FAIL because `merge_rule_events` does not exist.

- [ ] **Step 3: Implement the minimal aggregate**

```rust
pub(crate) fn merge_rule_events(mut candidates: Vec<RuleEvent>, gap_ms: u64) -> Vec<RuleEvent> {
    candidates.sort_by_key(|candidate| candidate.start_time_ms);
    let mut merged = Vec::new();
    for candidate in candidates {
        if let Some(active) = merged.last_mut().filter(|active: &&mut RuleEvent| {
            active.event_type == candidate.event_type
                && active.rule_version == candidate.rule_version
                && primary_class(active) == primary_class(&candidate)
                && candidate.start_time_ms.saturating_sub(active.end_time_ms) <= gap_ms
        }) { combine_rule_events(active, candidate); } else { merged.push(candidate); }
    }
    merged
}
```

`combine_rule_events` must preserve earliest start/latest end, append frames and objects, and calculate a detection-count-weighted mean confidence.

- [ ] **Step 4: Integrate aggregation before event persistence and run tests**

Replace the `for candidate in RuleEngine::new(rule).evaluate(&frames)` loop with the merged output and run:

`cargo test --manifest-path backend/Cargo.toml --test worker_contract merge`

Expected: PASS.

- [ ] **Step 5: Commit the focused change**

```bash
git add backend/src/worker.rs backend/tests/worker_contract.rs
git commit -m "feat: merge continuous rule events"
```

### Task 2: Select Representative Evidence Frames

**Files:**
- Modify: `backend/src/worker.rs`
- Test: `backend/tests/worker_contract.rs`

**Interfaces:**
- Consumes: `&[FrameDetection]` with a source path and detection confidence.
- Produces: `pub(crate) fn select_evidence_frames(frames: &[FrameDetection], sample_interval_ms: u64, max_frames: usize) -> Vec<(u64, &Path, Vec<Detection>)>`.

- [ ] **Step 1: Write failing selection tests**

```rust
#[test]
fn selects_first_peak_last_and_five_second_samples_without_duplicates() {
    let selected = select_evidence_frames(&frames_at([0, 500, 5_000, 7_000, 10_000]), 5_000, 12);
    assert_eq!(timestamps(&selected), vec![0, 500, 5_000, 10_000]);
}

#[test]
fn caps_selected_evidence_at_twelve_frames() {
    assert_eq!(select_evidence_frames(&many_unique_frames(), 0, 12).len(), 12);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract evidence`

Expected: FAIL because `select_evidence_frames` does not exist.

- [ ] **Step 3: Implement deterministic selection**

De-duplicate by `(timestamp_ms, frame_path)`, aggregate detections per frame, pick first/peak/last, then scan chronological candidates with a 5,000 ms sample interval. Sort by timestamp before returning and enforce `max_frames` while retaining the mandatory three representatives.

- [ ] **Step 4: Use selected evidence for `save_event_evidence` and run tests**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract evidence`

Expected: PASS.

- [ ] **Step 5: Commit the focused change**

```bash
git add backend/src/worker.rs backend/tests/worker_contract.rs
git commit -m "feat: retain representative event evidence"
```

### Task 3: Delete Event Evidence and Harden Media Responses

**Files:**
- Modify: `backend/src/storage.rs`
- Modify: `backend/src/api.rs`
- Test: `backend/tests/api_contract.rs`

**Interfaces:**
- Consumes: `MediaStorage`, parsed `Uuid`, and an existing delete-event route.
- Produces: `pub async fn delete_event_evidence(&self, event_id: Uuid) -> io::Result<()>`; JPEG responses with `Cache-Control: private, max-age=3600`.

- [ ] **Step 1: Write failing storage/API tests**

```rust
#[tokio::test]
async fn deleting_an_event_removes_its_evidence_directory() {
    // save evidence for the event, DELETE its API route, assert directory is absent
}

#[tokio::test]
async fn media_response_includes_private_cache_header() {
    let response = app.oneshot(Request::get("/media/evidence/id/frame.jpg").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.headers()[CACHE_CONTROL], "private, max-age=3600");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract evidence`

Expected: FAIL because evidence remains after deletion and the cache header is absent.

- [ ] **Step 3: Implement scoped cleanup and cache headers**

```rust
pub async fn delete_event_evidence(&self, event_id: Uuid) -> io::Result<()> {
    match tokio::fs::remove_dir_all(self.root.join("evidence").join(event_id.to_string())).await {
        Ok(()) | Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
```

Call it only after persistence reports the event was deleted; log cleanup failures rather than changing an already-successful delete response. Add the cache-control header to `get_media`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract evidence`

Expected: PASS.

- [ ] **Step 5: Commit the focused change**

```bash
git add backend/src/storage.rs backend/src/api.rs backend/tests/api_contract.rs
git commit -m "feat: clean event evidence on delete"
```

### Task 4: Recover Gracefully from Missing Evidence Images

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: `EvidenceFrame.image_url` and the existing active frame selection.
- Produces: an image recovery state keyed by `image_url`, with a retry button that re-renders the image.

- [ ] **Step 1: Write the failing UI test**

```tsx
fireEvent.error(screen.getByRole('img', { name: '00:00.0 的检测证据' }));
expect(await screen.findByText('证据文件不可用')).toBeInTheDocument();
fireEvent.click(screen.getByRole('button', { name: '重新加载证据图片' }));
expect(screen.getByRole('img', { name: '00:00.0 的检测证据' })).toBeInTheDocument();
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- --run --pool=threads --maxWorkers=1 src/App.test.tsx`

Expected: FAIL because image error state is not rendered.

- [ ] **Step 3: Implement recovery state**

Track failed URLs in component state. On `img` error, render a stage-local message and a button that removes the URL from the failed set. Reset the state when selecting another event or timeline point. Do not remove the timeline, detections, summary, or review controls.

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test -- --run --pool=threads --maxWorkers=1 src/App.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit the focused change**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles.css
git commit -m "feat: recover from missing evidence images"
```

### Task 5: Run Phase-A Regression Checks

**Files:**
- Modify: `docs/superpowers/plans/2026-08-25-event-quality-evidence.md` (mark verified checklist items)

- [ ] **Step 1: Run backend regression suite**

Run: `cargo test --manifest-path backend/Cargo.toml`

Expected: all backend unit and contract tests pass.

- [ ] **Step 2: Run frontend regression suite and production build**

Run: `npm test -- --run --pool=threads --maxWorkers=1`

Run: `npm run build`

Expected: all frontend tests pass and Vite produces a production build.

- [ ] **Step 3: Perform local manual acceptance**

Upload a video containing a continuous person sequence, inspect one merged event, verify the evidence cap/timeline and direct JPEG URL, delete the event, then confirm `media/evidence/<event-id>` no longer exists.

- [ ] **Step 4: Commit plan checklist updates**

```bash
git add docs/superpowers/plans/2026-08-25-event-quality-evidence.md
git commit -m "docs: verify event quality phase"
```
