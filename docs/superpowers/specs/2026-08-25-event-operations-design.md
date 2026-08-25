# Event Operations Design

## Goal

Implement the C phase for local event operations: review workflow, scalable filtered event access, CSV export, event association, and HTML reports.

## C1 Review workflow

Events support `unreviewed`, `confirmed`, `ignored`, `processing`, `resolved`, and `closed`. A review request contains the target status, optional reviewer, note, and disposition. Every accepted transition writes an immutable `event_review_history` record containing the old status, new status, reviewer, timestamp, note, and disposition. Existing confirm/ignore endpoints remain as compatibility wrappers.

The UI keeps the current event detail layout, adds a review dialog and history panel, disables duplicate submissions, reports API failures, and updates the status with a short transition state.

## C2 Query and export

`GET /api/v1/events/query` accepts optional `job_id`, `event_type`, `zone_key`, `class_name`, `status`, `severity`, `min_confidence`, `max_confidence`, `from_ms`, `to_ms`, `reviewer`, `page`, `page_size`, and `sort`. It returns `items`, `total`, `page`, and `page_size`. `GET /api/v1/events/export.csv` accepts the same filters and emits UTF-8 CSV. Existing `GET /api/v1/events` remains available.

Events persist `zone_key`; rules use their event type as the default zone key until a dedicated zone entity exists. Query indexes cover status/time, job/time, event type/time, and zone/status/time.

## C3 Association and reports

Events are associated by a stable group key derived from job, zone, rule, and overlapping track IDs. The API exposes the group key and related event IDs. `GET /api/v1/events/:id/report.html` renders a self-contained local HTML report with video name, event timeline, evidence image links, rule metadata, detection summary, and review conclusion. Rule-based summaries remain available when no LLM is configured.

## Compatibility and local constraints

Existing confirm/ignore clients continue to work. Historical events without review records show an empty history and without a zone show `未指定区域`. No Docker, external service, or LLM dependency is required.

## Acceptance criteria

- Review transitions and history survive API restart through MySQL.
- Query filters compose, pagination returns accurate totals, and CSV matches the active filters.
- A ten-thousand-row query uses indexed predicates and returns a bounded page rather than all rows.
- Related events and an HTML report are available for a selected event.
- Existing upload, worker, YOLO, region filtering, evidence, frontend tests, and builds continue to pass.
