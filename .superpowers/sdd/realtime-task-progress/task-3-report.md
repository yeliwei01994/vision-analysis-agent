# Task 3 Report — Realtime Job Progress SSE

Date: 2026-08-31

## Scope

Implemented Task 3 only:

- backend SSE route at `GET /api/v1/jobs/progress/stream`
- worker stage publishing for the declared seven stages
- frontend `EventSource` wrapper `api.subscribeJobProgress(...)`
- compatibility follow-up so frontend ETA helpers accept numeric SSE timestamps and percent-style progress payloads

## Changed Files

- `backend/Cargo.toml`
- `backend/Cargo.lock`
- `backend/src/api.rs`
- `backend/src/worker.rs`
- `backend/tests/job_progress_contract.rs`
- `frontend/src/api/client.ts`
- `frontend/src/api/client.test.ts`
- `frontend/src/types/events.ts`
- `frontend/src/features/jobProgress.ts`
- `frontend/src/features/jobProgress.test.ts`

## Red / Green History

### Backend SSE contract

Red:

- Added `progress_stream_returns_sse_content_type_and_json_events` to `backend/tests/job_progress_contract.rs`
- Ran:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract progress_stream_returns_sse_content_type_and_json_events
```

- Result: failed as expected with `404` because `/api/v1/jobs/progress/stream` did not exist yet

Green:

- Implemented SSE route in `backend/src/api.rs`
- Added broadcast-to-SSE mapping with event name `job-progress` and keep-alive frames
- Re-ran:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract progress_stream_returns_sse_content_type_and_json_events
```

- Result: passed

### Frontend EventSource wrapper

Red:

- Added `frontend/src/api/client.test.ts`
- Ran:

```powershell
npm --prefix frontend test -- src/api/client.test.ts
```

- Result: failed as expected with `TypeError: api.subscribeJobProgress is not a function`

Green:

- Implemented `api.subscribeJobProgress(onEvent, onStateChange)`
- Re-ran:

```powershell
npm --prefix frontend test -- src/api/client.test.ts
```

- Result: passed

### Frontend SSE payload compatibility

Red 1:

- Added numeric `updated_at` coverage in `frontend/src/features/jobProgress.test.ts`
- Ran:

```powershell
npm --prefix frontend test -- src/features/jobProgress.test.ts
```

- Result: failed with `expected null to be 20000`

Green 1:

- Updated `frontend/src/features/jobProgress.ts` to accept `updated_at` as string or number
- Re-ran the focused test
- Result: passed

Red 2:

- Added percent-style progress coverage for backend-style values (`25`, `50`)
- Ran:

```powershell
npm --prefix frontend test -- src/features/jobProgress.test.ts
```

- Result: failed with `expected +0 to be 10000`, then re-ran after checking math and confirmed the correct expected remaining time should be `20000`

Green 2:

- Normalized progress values greater than `1` inside the ETA helper
- Re-ran:

```powershell
npm --prefix frontend test -- src/features/jobProgress.test.ts
```

- Result: passed

## Verification

Focused checks:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract
```

- Passed: 4 tests

```powershell
npm --prefix frontend test -- src/api/client.test.ts
```

- Passed: 1 test

```powershell
npm --prefix frontend test -- src/features/jobProgress.test.ts
```

- Passed: 9 tests

Frontend full suite:

```powershell
npm --prefix frontend test
```

- Passed: 5 files, 27 tests

Frontend build:

```powershell
npm --prefix frontend run build
```

- Passed

Backend full suite:

```powershell
cargo test --manifest-path backend/Cargo.toml
```

- Partially passed through API/domain/job progress coverage
- Failed in `tests/persistence_contract.rs` at `annotated_video_fields_round_trip_on_video_jobs`
- Failure: `PoolTimedOut`
- This matches the pre-existing branch ledger note about an existing MySQL suite blocker and did not point at the Task 3 files

## Self-Review

- Confirmed the SSE route is registered before `/api/v1/jobs/:id`
- Confirmed SSE payloads use event name `job-progress`
- Confirmed the stream uses keep-alive frames and simply drops the broadcast receiver when the client disconnects
- Confirmed worker publishing covers: `preparing`, `reading`, `extracting_frames`, `detecting`, `analyzing_events`, `generating_playback`, `finalizing`
- Confirmed terminal completion/failure is emitted via `finalizing` after job state is updated to the terminal status
- Confirmed the frontend wrapper does not use `fetch` and therefore does not set `Content-Type`
- Confirmed cleanup removes listeners and closes the `EventSource`

## Concerns

- Worker stage progress currently preserves the coarse existing semantics by reusing `1`, `35`, `75`, and `100`; there is still no finer-grained progress within extraction/detection/playback stages.
- The backend full suite still has the unrelated `PoolTimedOut` persistence failure, so repository-wide backend verification is not completely green yet.
