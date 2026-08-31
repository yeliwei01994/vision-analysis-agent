# Task 2 Report

## Summary

Implemented the minimal backend progress contract for Task 2 in the assigned worktree:

- Added `JobStage` with snake-case serialization.
- Added serializable `JobProgressEvent`.
- Added `AppState::publish_job_progress(...)` and `AppState::subscribe_job_progress()`.
- Added per-job monotonically increasing progress sequences.
- Guarded `update_job(...)` so a terminal job cannot regress to a non-terminal processing update.
- Added focused contract coverage in `backend/tests/job_progress_contract.rs`.

## Changed Files

- Modified `backend/Cargo.toml`
- Modified `backend/src/domain.rs`
- Modified `backend/src/application.rs`
- Added `backend/tests/job_progress_contract.rs`

## TDD Log

### RED

Command:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract
```

First invalid run:

```text
error: no test target named `job_progress_contract` in default-run packages
```

Cause:

- The new test file was initially patched into the repo root instead of the assigned worktree.
- I corrected the file location and reran the focused test from the requested worktree.

Valid red run output:

```text
error[E0432]: unresolved import `vision_event_api::domain::JobStage`
error[E0599]: no method named `subscribe_job_progress` found for struct `AppState`
error[E0599]: no method named `publish_job_progress` found for struct `AppState`
error[E0599]: no method named `publish_job_progress` found for struct `AppState`
```

This is the expected missing-contract failure from the task brief.

### GREEN

Focused command:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract
```

Focused output:

```text
running 2 tests
test terminal_jobs_do_not_regress_to_processing_updates ... ok
test publish_job_progress_emits_snake_case_stage_and_monotonic_sequence ... ok

test result: ok. 2 passed; 0 failed
```

## Full Backend Verification

Command:

```powershell
cargo test --manifest-path backend/Cargo.toml
```

Observed result on both full-suite runs:

```text
test annotated_video_fields_round_trip_on_video_jobs ... FAILED

thread 'annotated_video_fields_round_trip_on_video_jobs' panicked at tests\persistence_contract.rs:53:80:
called `Result::unwrap()` on an `Err` value: PoolTimedOut
```

Notes:

- The new Task 2 contract test passed in the full suite.
- API and domain contract suites passed.
- The repeated failure was isolated to an existing MySQL-backed persistence integration test in `backend/tests/persistence_contract.rs`.
- I did not change persistence code as part of Task 2.

## Self-Review

- Kept the change additive to preserve `VideoJob` JSON compatibility.
- Did not modify API routes, frontend files, or worker behavior.
- Used Tokio broadcast and per-job sequence state as the smallest publisher contract that satisfies the brief.
- Applied the terminal-state regression guard in the existing `update_job(...)` path so older processing updates cannot overwrite `completed`, `failed`, or `cancelled`.

## Concerns

- Full backend verification is currently blocked by an existing/reproducible `PoolTimedOut` failure in `backend/tests/persistence_contract.rs::annotated_video_fields_round_trip_on_video_jobs`.

## Fix Round 1

### Reviewer Findings Addressed

1. `publish_job_progress(...)` now preserves the stored terminal snapshot instead of writing or emitting stale non-terminal progress after a job is already terminal.
2. Contract coverage now exercises the public publisher path and verifies terminal protection for `Completed`, `Failed`, and `Cancelled`.

### RED

Command:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract
```

Output:

```text
running 3 tests
test terminal_jobs_do_not_regress_to_processing_updates ... ok
test publish_job_progress_preserves_terminal_state_and_emits_stored_snapshot ... FAILED
test publish_job_progress_emits_snake_case_stage_and_monotonic_sequence ... ok

assertion `left == right` failed
  left: 25
 right: 100
```

This reproduced the stale event/stored-state mismatch on the public publisher path.

### GREEN

Command:

```powershell
cargo test --manifest-path backend/Cargo.toml --test job_progress_contract
```

Output:

```text
running 3 tests
test terminal_jobs_do_not_regress_to_processing_updates ... ok
test publish_job_progress_preserves_terminal_state_and_emits_stored_snapshot ... ok
test publish_job_progress_emits_snake_case_stage_and_monotonic_sequence ... ok

test result: ok. 3 passed; 0 failed
```

### Non-MySQL Backend Verification

Command:

```powershell
cargo test --manifest-path backend/Cargo.toml --lib --bins --test api_contract --test domain_contract --test job_progress_contract --test rule_engine --test upload_contract --test video_frames --test worker_contract --test yolo_contract -- --skip persisted_video_job_can_be_loaded_by_worker --skip worker_refreshes_rules_saved_by_the_api_process --skip failed_video_processing_is_persisted_to_mysql
```

Output summary:

```text
lib: 2 passed
api_contract: 19 passed
domain_contract: 2 passed
job_progress_contract: 3 passed
rule_engine: 8 passed
upload_contract: 3 passed
video_frames: 7 passed
worker_contract: 8 passed, 3 filtered out
yolo_contract: 3 passed
```

### Fix-Round Self-Review

- The fix stays inside `backend/src/application.rs` and does not change routes, worker orchestration, or frontend code.
- `publish_job_progress(...)` now emits from a single guarded snapshot when a job exists, which keeps event status/progress aligned with stored state.
- Terminal jobs short-circuit on the public publish path, so stale updates no longer regress progress for `Completed`, `Failed`, or `Cancelled`.
