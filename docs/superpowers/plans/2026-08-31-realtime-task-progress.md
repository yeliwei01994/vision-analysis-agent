# Realtime Task Progress UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将视频任务从单任务手动刷新体验升级为跨页面可恢复的实时任务中心，并在完成或失败时提供明确的结果与恢复操作。

**Architecture:** 后端保留现有 `VideoJob` 状态和百分比，增加阶段、提示、更新时间、预计剩余时间和顺序号，并通过 SSE 广播进度事件。前端建立以 `job_id` 为 key 的任务状态池，使用 SSE 优先、轮询兜底；事件检索页显示常驻任务条，视频任务页显示任务卡片，详情抽屉显示阶段时间线。

**Tech Stack:** React 18+, TypeScript, Vite, Vitest, Testing Library, Rust, Axum, Tokio。

**Spec:** `docs/superpowers/specs/2026-08-31-realtime-task-progress-design.md`

## Global Constraints

- 事件检索、筛选、分页、导出、审核、规则配置和视频回放行为必须保持兼容。
- 任务进度使用 8px 间距基准，事件列表与详情区域目标比例约为 `38.2% : 61.8%`。
- 确定性进度使用线性进度条；无法确定时显示阶段文案和不确定进度，不伪造百分比或剩余时间。
- 处理中阶段必须同时提供用户文案、可访问文本和非颜色状态表达。
- 所有主要操作点击区域不小于 44px；动效控制在 150～300ms，并支持 `prefers-reduced-motion`。
- SSE 断开时必须自动降级为 2～5 秒轮询，恢复后重新校准完整任务状态。
- 任务事件必须通过 `sequence` 防止旧消息覆盖新状态；终态不能被处理中消息覆盖。
- 每个行为变更先写失败测试，再写最小实现；每个任务完成后运行对应测试和全量回归测试。

## File Map

- Modify `frontend/src/types/events.ts`: 添加任务阶段、进度事件和任务展示状态类型。
- Modify `frontend/src/api/client.ts`: 添加任务进度 SSE 连接和必要的重新处理请求。
- Create `frontend/src/features/jobProgress.ts`: 纯函数状态归并、阶段文案、ETA 和连接策略。
- Create `frontend/src/features/jobProgress.test.ts`: 任务状态归并与 ETA 单元测试。
- Create `frontend/src/features/JobProgressBar.tsx`: 可访问的线性进度组件。
- Create `frontend/src/features/JobTaskCard.tsx`: 任务中心卡片和操作入口。
- Create `frontend/src/features/JobTaskDrawer.tsx`: 任务详情抽屉和阶段时间线。
- Modify `frontend/src/features/WorkspacePages.tsx`: 用任务卡片替换任务表格的主要展示，同时保留编辑和删除能力。
- Modify `frontend/src/App.tsx`: 统一任务状态池、订阅实时进展、渲染常驻任务条、完成后刷新事件。
- Modify `frontend/src/styles.css`: 任务条、卡片、抽屉、阶段时间线、响应式和减少动效样式。
- Modify `frontend/src/features/WorkspacePages.test.tsx` and `frontend/src/App.test.tsx`: 增加任务进度可见性和操作回归测试。
- Modify `backend/src/domain.rs`: 添加任务阶段和进度事件结构。
- Modify `backend/src/application.rs`: 添加任务进度发布、顺序号和订阅管理。
- Modify `backend/src/worker.rs`: 在关键处理阶段发布阶段文案和百分比。
- Modify `backend/src/api.rs`: 添加 SSE 任务进度路由和重新处理路由。
- Create `backend/tests/job_progress_contract.rs`: 后端状态推进、终态保护和 SSE 数据契约测试。

---

### Task 1: 建立前端任务进度领域模型

**Files:**
- Modify: `frontend/src/types/events.ts`
- Create: `frontend/src/features/jobProgress.ts`
- Test: `frontend/src/features/jobProgress.test.ts`

**Interfaces:**
- Produces `JobStage`, `JobProgressEvent`, `JobConnectionState`。
- Produces `mergeJobProgress(previous, next): JobProgressEvent`。
- Produces `jobStageLabel(stage): string`、`jobStatusLabel(status): string` 和 `estimateRemainingMs(history, current): number | null`。

- [ ] **Step 1: Write the failing tests**

```ts
it('accepts a newer progress event and keeps the newest stage', () => {
  const current = { job_id: 'job-1', status: 'processing', stage: 'reading', progress: 20, sequence: 2 };
  const next = { job_id: 'job-1', status: 'processing', stage: 'detecting', progress: 35, sequence: 3 };
  expect(mergeJobProgress(current, next)).toEqual(next);
});

it('rejects an older event and never regresses a terminal job', () => {
  const current = { job_id: 'job-1', status: 'completed', stage: 'finalizing', progress: 100, sequence: 8 };
  const older = { job_id: 'job-1', status: 'processing', stage: 'detecting', progress: 75, sequence: 7 };
  expect(mergeJobProgress(current, older)).toEqual(current);
});

it('does not invent an ETA without enough history', () => {
  const current = { job_id: 'job-1', status: 'processing', stage: 'detecting', progress: 35, sequence: 3 };
  expect(estimateRemainingMs([], current)).toBeNull();
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `npm --prefix frontend test -- src/features/jobProgress.test.ts`
Expected: FAIL because the progress functions and types do not exist yet.

- [ ] **Step 3: Write the minimal implementation**

Implement `mergeJobProgress` with these rules: ignore a different `job_id`, ignore lower `sequence`, keep a terminal current state over any non-terminal next state, and otherwise return `next`. Return `null` from `estimateRemainingMs` until two timestamped progress samples show positive progress; use linear extrapolation and clamp negative values to zero. Map every declared stage to the Chinese labels in the spec.

- [ ] **Step 4: Run the focused test and verify it passes**

Run: `npm --prefix frontend test -- src/features/jobProgress.test.ts`
Expected: PASS with all progress-domain tests green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/types/events.ts frontend/src/features/jobProgress.ts frontend/src/features/jobProgress.test.ts
git commit -m "feat: add frontend job progress model"
```

### Task 2: 增加后端任务阶段和事件发布契约

**Files:**
- Modify: `backend/src/domain.rs`
- Modify: `backend/src/application.rs`
- Create: `backend/tests/job_progress_contract.rs`

**Interfaces:**
- Produces `JobStage` enum with `Preparing`, `Reading`, `ExtractingFrames`, `Detecting`, `AnalyzingEvents`, `GeneratingPlayback`, `Finalizing`。
- Produces serializable `JobProgressEvent { job_id, status, stage, progress, message, updated_at, estimated_remaining_ms, sequence }`。
- Produces `AppState::publish_job_progress(job_id, stage, progress, message)` and a subscribe method returning a Tokio broadcast receiver。

- [ ] **Step 1: Write the failing contract tests**

```rust
#[tokio::test]
async fn progress_event_has_monotonic_sequence_and_serializable_stage() {
    let state = test_state();
    let mut receiver = state.subscribe_job_progress();
    state.publish_job_progress(job_id, JobStage::Reading, 20, "正在读取视频");
    state.publish_job_progress(job_id, JobStage::Detecting, 35, "正在进行目标检测");
    let first = receiver.recv().await.unwrap();
    let second = receiver.recv().await.unwrap();
    assert!(second.sequence > first.sequence);
    assert_eq!(second.stage.as_deref(), Some("detecting"));
}

#[test]
fn terminal_job_cannot_be_regressed_by_old_processing_update() {
    let state = test_state_with_completed_job(job_id);
    state.apply_job_progress(job_id, JobStatus::Processing, 75, Some(JobStage::Detecting));
    assert_eq!(state.job(job_id).unwrap().status, JobStatus::Completed);
    assert_eq!(state.job(job_id).unwrap().progress, 100);
}
```

- [ ] **Step 2: Run the focused backend tests and verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml --test job_progress_contract`
Expected: FAIL because the event type, publisher, subscriber, and monotonic update methods are absent.

- [ ] **Step 3: Write the minimal backend implementation**

Add a bounded `tokio::sync::broadcast::Sender<JobProgressEvent>` to `AppState`, initialize it in `AppState::new`, increment a per-job sequence number when publishing, and serialize stages as snake-case strings. The state update method must compare current terminal status and incoming sequence before modifying the in-memory job.

- [ ] **Step 4: Run focused and existing backend tests**

Run: `cargo test --manifest-path backend/Cargo.toml --test job_progress_contract`
Expected: PASS.

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all existing backend tests pass.

- [ ] **Step 5: Commit**

```bash
git add backend/src/domain.rs backend/src/application.rs backend/tests/job_progress_contract.rs
git commit -m "feat: publish job progress events"
```

### Task 3: 接入 Worker 阶段进度并提供 SSE 接口

**Files:**
- Modify: `backend/src/worker.rs`
- Modify: `backend/src/api.rs`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/types/events.ts`
- Test: `backend/tests/job_progress_contract.rs`

**Interfaces:**
- Backend route: `GET /api/v1/jobs/progress/stream` returns `text/event-stream` events containing JSON `JobProgressEvent`。
- Frontend API: `api.subscribeJobProgress(onEvent, onStateChange): () => void`。

- [ ] **Step 1: Write the failing API contract test**

```rust
#[tokio::test]
async fn progress_stream_returns_sse_content_type_and_json_events() {
    let response = test_app().get("/api/v1/jobs/progress/stream").await;
    assert_eq!(response.header("content-type"), "text/event-stream");
    publish_test_progress();
    let body = response.read_until_event().await;
    assert!(body.contains("event: job-progress"));
    assert!(body.contains("\"stage\":\"detecting\""));
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --test job_progress_contract progress_stream_returns_sse_content_type_and_json_events`
Expected: FAIL because the route and stream adapter do not exist.

- [ ] **Step 3: Implement the SSE route and Worker publishing**

Add the route before the parameterized `/api/v1/jobs/:id` route. Convert broadcast messages to `axum::response::sse::Event` with event name `job-progress`, JSON data, and keep-alive frames. In `worker.rs`, publish at the seven declared stages and preserve the existing 0/1/35/75/100 progress semantics until stage-specific values are available. On stream closure, drop the receiver without affecting processing.

- [ ] **Step 4: Implement the frontend subscription wrapper**

Use `EventSource('/api/v1/jobs/progress/stream')`; parse `event.data` as `JobProgressEvent`, call `onEvent`, call `onStateChange('connected' | 'reconnecting')`, and return a cleanup function that removes listeners and closes the source. Do not set `Content-Type` for EventSource.

- [ ] **Step 5: Run backend and frontend tests**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: PASS.

Run: `npm --prefix frontend test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/worker.rs backend/src/api.rs backend/tests/job_progress_contract.rs frontend/src/api/client.ts frontend/src/types/events.ts
git commit -m "feat: stream realtime job progress"
```

### Task 4: 实现前端 SSE/轮询状态池和上传任务闭环

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/features/jobProgress.ts`
- Test: `frontend/src/App.test.tsx`
- Test: `frontend/src/features/jobProgress.test.ts`

**Interfaces:**
- Produces `useJobProgress(jobs, onJobsChange)` or an equivalent App-local hook with `jobsById`, `connectionState`, `activeJobId`, `retryJob`, and `refreshAll`。
- `upload()` must set the created job immediately, start processing without awaiting a long-running processing request when the API returns an accepted task, and let the shared subscription update it.

- [ ] **Step 1: Write failing behavior tests**

```tsx
it('shows a newly uploaded task before processing completes', async () => {
  render(<App />);
  await user.upload(screen.getByLabelText('导入视频任务'), videoFile);
  expect(await screen.findByText('监控视频-01.mp4')).toBeVisible();
  expect(screen.getByText(/正在|等待/)).toBeVisible();
});

it('updates task progress from a realtime event without manual refresh', async () => {
  render(<App />);
  emitJobProgress({ job_id: 'job-1', status: 'processing', stage: 'detecting', progress: 35, sequence: 3 });
  expect(await screen.findByText('正在进行目标检测')).toBeVisible();
  expect(screen.getByText('35%')).toBeVisible();
});
```

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `npm --prefix frontend test -- src/App.test.tsx`
Expected: FAIL because the app has no shared progress subscription and no stage-based rendering.

- [ ] **Step 3: Implement the state pool and reconnect fallback**

Subscribe once at App level. Merge incoming events through `mergeJobProgress`. When the EventSource enters `error`, set the visible connection state to reconnecting and start a single interval between 2 and 5 seconds that calls `api.listJobs`; stop the interval when SSE opens again. Clear the subscription and interval in the effect cleanup. Keep the selected event stable during job refreshes unless no selected event remains.

- [ ] **Step 4: Fix the upload flow**

Immediately call `setJobs` with the returned job. If the API returns `pending`, call `api.processVideo` without blocking the UI; handle the returned job if it resolves, then let SSE or polling reconcile the final state. Keep loading limited to the upload operation so users can continue browsing after the file is accepted.

- [ ] **Step 5: Implement retry behavior**

Expose `retryJob(id)` that calls the process endpoint for failed or cancelled jobs, updates the local job to processing/pending from the response, clears the previous error banner, and leaves the task visible at the top of the active list.

- [ ] **Step 6: Run focused and full frontend tests**

Run: `npm --prefix frontend test -- src/App.test.tsx src/features/jobProgress.test.ts`
Expected: PASS.

Run: `npm --prefix frontend test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/App.tsx frontend/src/api/client.ts frontend/src/features/jobProgress.ts frontend/src/App.test.tsx frontend/src/features/jobProgress.test.ts
git commit -m "feat: keep job progress in sync across the app"
```

### Task 5: 构建任务进度组件和任务中心卡片

**Files:**
- Create: `frontend/src/features/JobProgressBar.tsx`
- Create: `frontend/src/features/JobTaskCard.tsx`
- Modify: `frontend/src/features/WorkspacePages.tsx`
- Modify: `frontend/src/styles.css`
- Test: `frontend/src/features/WorkspacePages.test.tsx`

**Interfaces:**
- `JobProgressBar({ progress, indeterminate, label })` renders an accessible `progressbar` with `aria-valuenow` only for determinate progress.
- `JobTaskCard({ job, progressEvent, onOpen, onRetry, onCancel, onEdit, onDelete })` renders status, stage, progress, update time, ETA, and actions.
- `JobsPage` accepts `progressById`, `connectionState`, `onOpenJob`, `onRetryJob` in addition to current `jobs` and `onRefresh` props.

- [ ] **Step 1: Write failing component tests**

```tsx
it('renders stage, progress, update time, and the correct recovery action', () => {
  render(<JobTaskCard job={failedJob} progressEvent={failedEvent} onRetry={vi.fn()} />);
  expect(screen.getByText('视频读取失败')).toBeVisible();
  expect(screen.getByText('重新处理')).toBeEnabled();
  expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '35');
});

it('uses an indeterminate progress bar when progress is unknown', () => {
  render(<JobProgressBar progress={null} indeterminate label="等待处理" />);
  expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
});
```

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `npm --prefix frontend test -- src/features/WorkspacePages.test.tsx`
Expected: FAIL because the components and new task center props do not exist.

- [ ] **Step 3: Implement the components**

Use the existing dark theme tokens. Show a 44px-or-larger action target, Chinese user-facing status labels, stage message, `最近更新`, and ETA only when `estimateRemainingMs` returns a value. Use `button` elements for all actions and include accessible labels containing the filename.

- [ ] **Step 4: Replace the primary jobs table presentation**

Render active cards first and completed jobs as compact rows below. Keep the existing edit modal, delete confirmation, filename validation, and manual refresh button. Disable delete for processing jobs. Show a connection notice above the list only while reconnecting.

- [ ] **Step 5: Add responsive and reduced-motion styles**

Use the 8px spacing scale, card padding 16/24px, 150–300ms transitions, and a `@media (prefers-reduced-motion: reduce)` override that disables progress shimmer and status transitions. Ensure the task center wraps at viewport widths below 900px without horizontal scrolling.

- [ ] **Step 6: Run focused and full frontend tests**

Run: `npm --prefix frontend test -- src/features/WorkspacePages.test.tsx`
Expected: PASS.

Run: `npm --prefix frontend run build`
Expected: TypeScript compilation and Vite build exit with code 0.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/features/JobProgressBar.tsx frontend/src/features/JobTaskCard.tsx frontend/src/features/WorkspacePages.tsx frontend/src/styles.css frontend/src/features/WorkspacePages.test.tsx
git commit -m "feat: add realtime job task center"
```

### Task 6: 增加事件检索页常驻任务条、任务详情抽屉和结果反馈

**Files:**
- Create: `frontend/src/features/JobTaskDrawer.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles.css`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- `JobTaskDrawer({ job, progressEvent, onClose, onRetry, onCancel, onViewEvents, onViewPlayback })` renders a dialog with stage timeline, recent progress, and contextual actions.
- `JobSummaryBar` may remain App-local or be extracted beside the drawer; it consumes the same `progressById` state and must never start its own polling or SSE connection.

- [ ] **Step 1: Write failing interaction tests**

```tsx
it('shows the active task summary on the event search page', async () => {
  render(<App />);
  expect(await screen.findByText(/目标检测中/)).toBeVisible();
  expect(screen.getByRole('button', { name: /查看任务详情/ })).toBeVisible();
});

it('offers result actions after a task completes', async () => {
  render(<App />);
  emitJobProgress({ job_id: 'job-1', status: 'completed', stage: 'finalizing', progress: 100, sequence: 9 });
  expect(await screen.findByText(/已生成.*事件/)).toBeVisible();
  expect(screen.getByRole('button', { name: /查看事件/ })).toBeEnabled();
});
```

- [ ] **Step 2: Run focused tests and verify they fail**

Run: `npm --prefix frontend test -- src/App.test.tsx`
Expected: FAIL because the summary bar, drawer, stage timeline, and result actions do not exist.

- [ ] **Step 3: Implement the summary bar and drawer**

Render the summary bar between the header and metrics. Render only the most relevant active or recently completed task. The drawer must use `role="dialog"`, `aria-modal="true"`, a visible close button, keyboard escape handling, and focus-safe controls. Use one shared task state source.

- [ ] **Step 4: Implement completion and failure feedback**

On completion, refresh jobs and events, show event count when available, and expose `查看事件` plus `播放检测回放` when the annotated video is ready. On failure, show the failure stage, user-facing reason, and `重新处理`. Do not show fake event counts when the API has not returned them.

- [ ] **Step 5: Run focused and full frontend tests**

Run: `npm --prefix frontend test -- src/App.test.tsx`
Expected: PASS.

Run: `npm --prefix frontend test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/features/JobTaskDrawer.tsx frontend/src/App.tsx frontend/src/styles.css frontend/src/App.test.tsx
git commit -m "feat: surface task progress in event search"
```

### Task 7: 完整验证、响应式检查和文档同步

**Files:**
- Modify: `frontend/src/App.test.tsx` only if final regression coverage exposes a missing assertion.
- Modify: `README.md` or an existing developer document only if the SSE endpoint needs local setup instructions.

- [ ] **Step 1: Run the full frontend verification**

Run: `npm --prefix frontend test`
Expected: all frontend tests pass with no unhandled errors.

Run: `npm --prefix frontend run build`
Expected: TypeScript and Vite build exit with code 0.

- [ ] **Step 2: Run the full backend verification**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: all backend tests pass.

- [ ] **Step 3: Run the existing project check**

Run: `powershell -ExecutionPolicy Bypass -File scripts/check.ps1`
Expected: project check completes successfully without introducing new warnings or failures.

- [ ] **Step 4: Verify the responsive and accessibility requirements**

Run the frontend at viewport widths 375px, 768px, 1024px, and desktop width. Verify no horizontal scrolling, all task controls are keyboard reachable, dialog close/escape works, status is readable without color, and reduced motion removes shimmer/transitions.

- [ ] **Step 5: Verify real-time behavior manually**

Start the backend and frontend using the repository's existing development script. Upload a video, switch between 事件检索 and 视频任务, refresh the browser during processing, disconnect the SSE endpoint, and confirm that the visible task state falls back to polling and later reconciles with the server.

- [ ] **Step 6: Review the diff and commit the verification/documentation changes**

Run: `git diff --check`
Expected: no whitespace errors.

Run: `git status --short`
Expected: only intended implementation files and pre-existing user files are present.

```bash
git add frontend backend README.md
git commit -m "test: verify realtime task progress experience"
```

## Execution Notes

- Execute tasks in order because Tasks 2–3 define the backend event contract consumed by Task 4, and Tasks 5–6 consume the shared frontend state pool from Task 4.
- If the existing backend process endpoint is synchronous in a local configuration, keep the UI responsive by treating the upload response as accepted work and relying on the shared `/api/v1/jobs` refresh until the endpoint behavior is made asynchronous.
- Do not add a general-purpose event bus, notification center, browser notifications, or a full log search system in this iteration.
