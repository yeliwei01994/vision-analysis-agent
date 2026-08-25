# Event Quality and Evidence Design

## Scope

This design implements phase A of the upgrade roadmap for newly processed videos. It reduces duplicate events, keeps evidence useful without retaining every frame indefinitely, removes evidence together with its event, and makes missing media understandable in the UI.

Existing events are not backfilled or rewritten. Existing JSON event fields and media URLs remain valid.

## Goals

- Turn continuous detections of one class under one rule into a single reviewable event.
- Retain enough evidence to understand the start, peak, and end of an event.
- Bound evidence files to at most 12 images per event.
- Keep deletion of the database event and its local evidence directory consistent.
- Preserve graceful rendering when an old or missing evidence file is requested.

## Non-goals

- Object tracking, zone rules, video clips, image annotation rendering, object storage, and historical-event migration are deferred to later roadmap phases.
- Because YOLO does not currently emit stable track IDs, merging is class-based rather than identity-based. Tracking will refine this later.

## Event Merge Model

The worker evaluates rule candidates after YOLO has completed all extracted frames. Candidates are merged when all of the following values match:

- `job_id`;
- `event_type`;
- primary detected class name;
- `rule_version`;
- the gap between the next candidate's start and the active aggregate's end is no more than 3,000 ms.

Candidates outside that time gap start a new aggregate. The resulting event has the earliest start time and latest end time, contains every contributing detection for accurate counts, and uses the arithmetic mean of contributing candidate confidences as its `confidence` value.

The aggregate's evidence source is the de-duplicated set of `(timestamp_ms, source frame path)` entries across all its candidates. Detections from the same source frame are combined before saving, so one timestamp never creates duplicate JPEG files.

## Evidence Selection

Evidence selection occurs before `MediaStorage` copies JPEGs. The selection is deterministic and chronological in its final output:

1. first source frame;
2. source frame containing the highest-confidence detection (earlier timestamp wins a tie);
3. last source frame;
4. additional source frames at least 5,000 ms after the previously selected sampling frame, in chronological order;
5. if the unique selection exceeds 12 frames, retain first, peak, last, then earliest remaining sampled frames until the cap is reached.

The representative frame becomes `thumbnail_url`. `Evidence.frames` and `frame_urls` contain exactly the saved selected frames. This retains the existing frontend contract while reducing per-event disk growth.

## Storage and Deletion

`MediaStorage` gains two focused responsibilities:

- save selected evidence as it does today;
- delete `media/evidence/<event-id>` only after the persistence layer confirms that the event row was deleted.

Deletion treats a missing evidence directory as successful. It rejects no user-controlled path: event IDs are parsed UUIDs and the storage layer always constructs the directory below its configured media root.

If evidence cleanup fails after the database row is deleted, the API returns success because the user-requested event deletion completed, but records the cleanup failure in the server log. A later orphan-cleanup task in phase D can recover any such files.

## Media Reliability and UI Behaviour

The media route continues to serve only sanitized paths below the configured root. For JPEG evidence it returns `image/jpeg`, `Cache-Control: private, max-age=3600`, and a 404 JSON error when the file is absent.

The event detail view handles image `onError` by replacing the broken image with “证据文件不可用” and a retry action. It must continue to show the selected timestamp, detections, review controls, and summary; an unavailable image must not make the event unusable.

## Interfaces

- `worker::merge_rule_events(job_id, candidates, gap_ms)` produces merged rule events and is unit-testable without MySQL or YOLO.
- `worker::select_evidence_frames(frames, sample_interval_ms, max_frames)` produces a de-duplicated representative sequence and is unit-testable using synthetic frame metadata.
- `MediaStorage::delete_event_evidence(event_id)` removes only the event's evidence directory.
- The delete-event API keeps its current HTTP path and response shape; it additionally invokes evidence cleanup after a successful event-row delete.

## Verification

- Worker tests prove ten seconds of continuous same-class detections become one event and a gap over three seconds creates two events.
- Evidence tests prove first/peak/last selection, 5-second sampling, frame de-duplication, chronological output, and the 12-frame cap.
- Storage tests prove evidence directory removal and tolerate a missing directory.
- API tests prove a deleted event no longer has an evidence directory and media responses use the expected JPEG cache headers and 404 behaviour.
- Frontend tests prove an image load failure presents the recovery message and retry control.

## Acceptance Criteria

For a newly uploaded video with a continuous person detection sequence, the event stream contains one `person_stay` event per sequence separated by more than three seconds. Its detail page can show no more than 12 chronological evidence frames, including the first, peak-confidence, and last frame. Deleting that event removes its row and local evidence directory, while old event records and old evidence URLs still render safely.
