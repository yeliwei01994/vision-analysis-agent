# Event Evidence Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make local YOLO events understandable and reviewable through persisted frame evidence, grouped Chinese summaries, an interactive evidence timeline, and visible review feedback.

**Architecture:** The Rust worker copies only rule-matching sampled frames into `MEDIA_ROOT/evidence/<event-id>/` and stores URLs, timestamps, and real detection boxes in the event JSON. Axum exposes those files under `/media`; the React client derives presentation summaries and groups separately from persisted IDs, then renders the selected evidence frame and timeline.

**Tech Stack:** Rust 2021, Axum 0.7, Tokio, SQLx/MySQL JSON, React, TypeScript, Vitest, Testing Library, plain CSS.

**Spec:** `docs/superpowers/specs/2026-08-25-event-evidence-experience-design.md`

## Global Constraints

- Keep uploaded video and evidence images as filesystem files under `MEDIA_ROOT`; do not store image binary data in MySQL.
- Keep event API routes and review/delete behavior backward compatible.
- Use only real YOLO `bbox`, confidence, and timestamp data; never render placeholder detection boxes when evidence exists.
- Old event JSON without `frames` must deserialize and show a deliberate no-evidence UI.
- All new behavior is test-first; run targeted tests after every task.

---

### Task 1: Persist frame-level evidence for emitted rule events

**Files:**
- Modify: `backend/src/domain.rs:65-114`
- Modify: `backend/src/rules.rs:5-90`
- Modify: `backend/src/storage.rs:1-43`
- Modify: `backend/src/worker.rs:1-121`
- Modify: `backend/tests/worker_contract.rs`

**Interfaces:**
- Consumes: `FrameDetection { timestamp_ms, detection }` and extracted JPEG paths.
- Produces: `EvidenceFrame { timestamp_ms: u64, image_url: String, detections: Vec<Detection> }` stored in `Event.evidence.frames`.

- [ ] **Step 1: Write the failing worker evidence test**

Add a test using a temporary media root and JPEG-named source file. Call `MediaStorage::save_event_evidence` and assert it copies the source under `evidence/<event-id>/`, returns a matching timestamp/detection, and makes its first URL the thumbnail.

```rust
#[tokio::test]
async fn event_evidence_copies_matching_frames_under_media_root() {
    let temp = tempfile::tempdir().unwrap();
    let storage = MediaStorage::new(temp.path().join("media"));
    let evidence = storage.save_event_evidence(Uuid::new_v4(), &[(500, frame_path, vec![detection])]).await.unwrap();
    assert_eq!(evidence.frames[0].timestamp_ms, 500);
    assert_eq!(evidence.thumbnail_url, Some(evidence.frames[0].image_url.clone()));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract event_evidence_copies_matching_frames_under_media_root -- --nocapture`

Expected: compile failure because `EvidenceFrame`, `Evidence.frames`, or `save_event_evidence` is absent.

- [ ] **Step 3: Add backward-compatible evidence types and storage copy helper**

Add `EvidenceFrame` and `#[serde(default)] pub frames: Vec<EvidenceFrame>` in `domain.rs`. Add `save_event_evidence(event_id, frames)` in `storage.rs`: create the event folder, copy each source to a numbered JPEG, and return `/media/evidence/<event-id>/<filename>` URLs.

```rust
pub async fn save_event_evidence(
    &self, event_id: Uuid, frames: &[(u64, &Path, Vec<Detection>)]
) -> std::io::Result<Evidence> { /* create directory, copy, construct frames */ }
```

- [ ] **Step 4: Preserve frame paths through rule evaluation and call the helper**

Extend `FrameDetection` with `frame_path: PathBuf`. Let `RuleEvent` retain matched frame records while preserving its flattened `objects` compatibility field. Attach the source frame path in `process_video`. After constructing each `Event`, save its evidence and log copy failures while leaving evidence empty rather than failing the job.

```rust
event.evidence = match state.storage.save_event_evidence(event.id, &candidate.frames).await {
    Ok(evidence) => evidence,
    Err(error) => { eprintln!("failed to save evidence for {}: {error}", event.id); Evidence::default() }
};
```

- [ ] **Step 5: Run targeted worker tests**

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract -- --nocapture`

Expected: PASS, including the new copy test and existing worker persistence tests.

- [ ] **Step 6: Commit**

```bash
git add backend/src/domain.rs backend/src/rules.rs backend/src/storage.rs backend/src/worker.rs backend/tests/worker_contract.rs
git commit -m "feat: persist event frame evidence"
```

### Task 2: Serve local evidence images safely from the API

**Files:**
- Modify: `backend/src/api.rs:1-75`
- Modify: `backend/tests/api_contract.rs`

**Interfaces:**
- Consumes: `AppState.storage.root()` and a relative path after `/media/`.
- Produces: `GET /media/evidence/<event-id>/<frame>.jpg` returning a JPEG, with absent/unsafe paths rejected.

- [ ] **Step 1: Write failing API tests**

Create a temporary `evidence/event-1/frame-0001.jpg`; request it via the router and assert `200 OK` and `image/jpeg`. Request `/media/../Cargo.toml` and assert it is not successful.

```rust
let response = router(state).oneshot(
    Request::get("/media/evidence/event-1/frame-0001.jpg").body(Body::empty()).unwrap()
).await.unwrap();
assert_eq!(response.status(), StatusCode::OK);
assert_eq!(response.headers()[CONTENT_TYPE], "image/jpeg");
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract media_ -- --nocapture`

Expected: `404`, because no media route exists.

- [ ] **Step 3: Add constrained media route**

Add `.route("/media/*path", get(get_media))`. Reject empty, absolute, parent, and non-normal path components. Join only accepted components to `storage.root()`; asynchronously read bytes; map missing files to 404; serve JPEG content type.

```rust
async fn get_media(State(state): State<AppState>, Path(path): Path<String>) -> Result<Response, ApiError> {
    let relative = safe_media_path(&path).ok_or(ApiError::NotFound)?;
    let bytes = tokio::fs::read(state.storage.root().join(relative)).await.map_err(|_| ApiError::NotFound)?;
    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
```

- [ ] **Step 4: Re-run media tests**

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract media_ -- --nocapture`

Expected: PASS for retrieval and traversal rejection.

- [ ] **Step 5: Commit**

```bash
git add backend/src/api.rs backend/tests/api_contract.rs
git commit -m "feat: serve persisted event evidence"
```

### Task 3: Add frontend presentation utilities and tests

**Files:**
- Create: `frontend/src/features/eventPresentation.ts`
- Create: `frontend/src/features/eventPresentation.test.ts`
- Modify: `frontend/src/types/events.ts:1-6`

**Interfaces:**
- Consumes: `EventItem`, `Detection`, `EvidenceFrame`.
- Produces: `displayEventType`, `detectionSummary`, `fallbackAnalysis`, `groupEvents`, and `formatPreciseTime`.

- [ ] **Step 1: Write failing pure utility tests**

Cover `person_stay → 人员停留`, 11 repeated people, average confidence, analysis fallback, precise decimal timestamps, and grouping events sharing `job_id + event_type + rule_version`.

```ts
expect(detectionSummary(personEvent)).toBe('人员 11 次检测 · 平均置信度 34%');
expect(displayEventType('person_stay')).toBe('人员停留');
expect(fallbackAnalysis(event)).toContain('视频前 6 秒检测到人员停留');
expect(groupEvents([first, duplicate, other])).toHaveLength(2);
```

- [ ] **Step 2: Verify the test fails**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/features/eventPresentation.test.ts`

Expected: module-not-found for `eventPresentation`.

- [ ] **Step 3: Implement types and pure utilities**

Add optional/default-compatible `Evidence.frames`. Keep a presentation-only dictionary for known rule and COCO names. Calculate averages from detection confidences, falling back to event confidence only when there are no detections. Do not mutate input arrays; order grouped occurrences by start time.

```ts
export function detectionSummary(event: EventItem): string {
  const average = event.objects.length
    ? event.objects.reduce((sum, item) => sum + item.confidence, 0) / event.objects.length
    : event.confidence;
  return `\${displayClassName(event.objects[0]?.class_name ?? 'unknown')} \${event.objects.length} 次检测 · 平均置信度 \${(average * 100).toFixed(0)}%`;
}
```

- [ ] **Step 4: Verify utilities pass and commit**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/features/eventPresentation.test.ts`

Expected: PASS.

```bash
git add frontend/src/types/events.ts frontend/src/features/eventPresentation.ts frontend/src/features/eventPresentation.test.ts
git commit -m "feat: add event presentation summaries"
```

### Task 4: Render grouped evidence cards and interactive evidence timeline

**Files:**
- Modify: `frontend/src/App.tsx:1-83`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/styles.css`

**Interfaces:**
- Consumes: Task 3 utilities plus `EvidenceFrame`.
- Produces: grouped cards, occurrence selection, real evidence image/boxes, accessible timeline, and missing-evidence state.

- [ ] **Step 1: Write failing UI tests**

Update fixtures with two evidence frames and two related events. Assert a single group card displays `人员停留`, `rule-v1`, and `人员 2 次检测`. Assert a real evidence image URL, clicking `证据帧 00:00.5` switches image/timestamp, and old events render `暂无可用抽帧证据`.

```tsx
fireEvent.click(await screen.findByRole('button', { name: '证据帧 00:00.5' }));
expect(screen.getByRole('img', { name: '00:00.5 的检测证据' }))
  .toHaveAttribute('src', '/media/evidence/event-1/frame-0002.jpg');
```

- [ ] **Step 2: Verify focused UI tests fail**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/App.test.tsx`

Expected: raw rule ID and placeholder-evidence assertions fail.

- [ ] **Step 3: Refactor the event view and render only real evidence**

Retain navigation/upload/delete flows. Add selected occurrence and selected frame state, resetting the active frame on event change. Card thumbnails use an `img` only when the stored URL exists. Box overlays use the exact bbox values as percentage styles; each timeline control uses `aria-pressed`.

```tsx
{activeFrame ? <div className="evidence-stage">
  <img src={activeFrame.image_url} alt={`\${formatPreciseTime(activeFrame.timestamp_ms)} 的检测证据`} />
  {activeFrame.detections.map((detection, index) => <span key={index} className="detection-box" style={boxStyle(detection.bbox)} />)}
</div> : <div className="evidence-empty">暂无可用抽帧证据</div>}
```

- [ ] **Step 4: Replace visual placeholders with accessible dark-theme styles**

Remove the fake stage pseudo-art. Add image fitting, detection overlay labels, rule metadata, group count, occurrence selector, timeline active/focus styles, and no-evidence styling. Keep existing dark design language and add focus-visible indicators.

- [ ] **Step 5: Verify UI and commit**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/App.test.tsx frontend/src/features/eventPresentation.test.ts`

Expected: PASS, including existing review/delete/navigation tests.

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles.css
git commit -m "feat: render grouped event evidence timeline"
```

### Task 5: Add deterministic analysis fallback and review feedback

**Files:**
- Modify: `frontend/src/App.tsx:35-83`
- Modify: `frontend/src/App.test.tsx`
- Modify: `frontend/src/styles.css`

**Interfaces:**
- Consumes: review responses and rejected review promises.
- Produces: `role="status"` success feedback, retained event state on failure, and motion-safe status transitions.

- [ ] **Step 1: Write failing tests**

Use `analysis: null` and assert deterministic Chinese summary, not waiting text. Confirm should show `事件已确认`. A rejected ignore request must show its error and retain `待复核`.

```tsx
apiMock.ignoreEvent.mockRejectedValueOnce(new Error('请求失败 (500)'));
fireEvent.click(screen.getByRole('button', { name: '忽略' }));
expect(await screen.findByRole('alert')).toHaveTextContent('请求失败 (500)');
expect(screen.getByText('待复核')).toBeInTheDocument();
```

- [ ] **Step 2: Verify review tests fail**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/App.test.tsx`

Expected: no success notice and no fallback analysis text.

- [ ] **Step 3: Implement feedback state and fallback analysis**

Add `feedback` independently from `error`; clear both at action start, set success text after API resolution, and retain the existing error on rejection. Render `event.analysis ?? fallbackAnalysis(event)` consistently in the stream and detail panel.

```tsx
const [feedback, setFeedback] = useState('');
setFeedback(action === 'confirm' ? '事件已确认' : '事件已忽略');
```

- [ ] **Step 4: Add motion-safe status styles**

Apply a 160ms transition to status badges, action results, and feedback; inside `prefers-reduced-motion: reduce`, disable those transitions.

- [ ] **Step 5: Run final verification and commit**

Run: `npm test -- --run --pool=threads --maxWorkers=1 frontend/src/App.test.tsx frontend/src/features/eventPresentation.test.ts`

Run: `npm run build`

Run: `cargo test --manifest-path backend/Cargo.toml --test worker_contract -- --nocapture`

Run: `cargo test --manifest-path backend/Cargo.toml --test api_contract media_ -- --nocapture`

Expected: all listed focused tests and the production build PASS. Record unrelated pre-existing upload-limit failures separately rather than masking them.

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/styles.css
git commit -m "feat: add review feedback and analysis fallback"
```

