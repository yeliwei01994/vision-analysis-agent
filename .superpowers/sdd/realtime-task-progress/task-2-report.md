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
