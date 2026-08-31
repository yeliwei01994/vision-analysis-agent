# Task 4 Report — 2026-08-31

## Scope

Implemented Task 4 in the frontend App layer using the existing Task 1 and Task 3 interfaces:

- `frontend/src/App.tsx`
- `frontend/src/App.test.tsx`
- `frontend/src/features/jobProgress.ts`
- `frontend/src/features/jobProgress.test.ts`

Left `frontend/src/api/client.ts` unchanged after review because Task 3's `api.subscribeJobProgress(onEvent, onStateChange)` contract already matched the Task 4 requirements.

## TDD Log

### Red

Added failing App tests first for:

- immediate uploaded job visibility in the shared task list
- realtime job progress updates merged into the app-level state

Command:

```bash
npm --prefix frontend test -- src/App.test.tsx
```

Observed expected failures:

- uploaded job did not appear in the task list after upload acceptance
- `api.subscribeJobProgress` was never called

### Captured hanging test and fix

During the first focused green run, the suite hung because an older App test still expected the removed per-job `getJob` polling loop, enabled fake timers, then failed before reaching `vi.useRealTimers()`. I replaced that outdated test with a reconnect fallback test that matches Task 4 behavior and added timer reset protection in `afterEach`.

### Green

Added unit coverage for the pure App-level pool helpers:

- apply realtime progress onto a `VideoJob`
- preserve the active in-flight job when a stale `listJobs` snapshot lags
- choose stage/status copy for the active job summary

Implemented:

- one App-level `job_id`-keyed pool with `jobsById`, `progressById`, `activeJobId`, and connection state
- one App-level SSE subscription
- `mergeJobProgress`-based event reconciliation
- reconnect fallback polling every 3 seconds while SSE is reconnecting
- immediate upload insertion plus non-blocking `processVideo`
- shared job refresh that preserves the current event selection when events reload
- `retryJob(id)` App-level behavior for failed/cancelled jobs

## Verification

Focused frontend tests:

```bash
npm --prefix frontend test -- src/App.test.tsx src/features/jobProgress.test.ts
```

Result: PASS (`2` files, `29` tests)

Full frontend tests:

```bash
npm --prefix frontend test
```

Result: PASS (`5` files, `32` tests)

Build:

```bash
npm --prefix frontend run build
```

Result: PASS (`tsc -b && vite build`)

## Self-review

- The App now owns a single shared job pool and no component opens its own SSE or fallback polling loop.
- Upload no longer blocks on background processing, so navigation remains usable after acceptance.
- The reconnect fallback is bounded to one 3-second interval and is cleared on reconnection and unmount.
- Event selection preservation is handled when events are refreshed after terminal job transitions.
- `retryJob(id)` is implemented in the App layer for downstream consumers, but this task intentionally did not add new retry UI because Tasks 5–6 own the task card/drawer surfaces.

## Concerns

- No Task 5/6 UI exists yet in this worktree, so the new App-level `retryJob` behavior is ready for later wiring but is not directly user-triggerable from the current interface.
- `frontend/src/api/client.ts` was listed in the task brief, but no code change was needed there after validating that Task 3 already provided the required SSE adapter contract.
