# Zone Rules Design

## Scope

This design implements the first business-rule subsystem: zone intrusion, person dwell, and occupancy limit. It includes durable rule configuration, a dual-background geometry editor, Worker evaluation, and event snapshots for newly processed videos.

Line crossing, stable object tracking, shared zone libraries, and historical-event migration are intentionally deferred.

## Rule Model

Each rule stores the current fields plus:

- `geometry`: `{ "type": "polygon", "points": [[x, y], ...] }`, where each coordinate is in the inclusive `0.0..=1.0` range;
- `threshold`: a positive whole number for `person_count_limit`, unused for other initial rules;
- `enabled`: whether the Worker evaluates the rule;
- `version`: updated whenever its saved configuration changes.

Rectangles are an editor convenience only. They are serialized as a four-point polygon, giving all three initial rule types one geometry representation. Each rule owns one geometry; zones are not yet shared between rules.

Rules persist in MySQL and are loaded into the application rule registry at API startup. The existing `person_stay` rule remains valid with no geometry until a user saves its zone configuration.

## Geometry Editor

The rule editor uses one fixed 16:9 canvas and offers two interchangeable backgrounds:

- blank canvas, available without any uploaded video;
- selected uploaded video’s first saved evidence frame, when one is available.

Changing background never changes geometry coordinates. Users can draw a rectangle or a polygon, move polygon points, reset the shape, configure the numeric parameters, enable or disable the rule, and save.

The client rejects a polygon with fewer than three points, points outside the canvas, a missing shape for a zone rule, non-positive count thresholds, and invalid confidence/duration values. If the selected evidence image cannot load, the editor falls back to the blank canvas and leaves the geometry editable.

## Worker Evaluation

For every YOLO detection satisfying the rule class and confidence threshold, the Worker calculates the bottom-center of its bounding box. The detection is inside the zone when that point lies in the polygon, with boundary points treated as inside.

- `person_enter_zone`: emits a candidate for matching detections within the zone.
- `person_stay`: evaluates only within-zone detections and uses the existing minimum-duration parameter.
- `person_count_limit`: counts matching detections inside the zone in each extracted frame and emits a candidate when count is greater than `threshold`.

All generated candidates continue through the phase-A merge and representative-evidence pipeline. Every created event stores a geometry and rule-parameter snapshot inside its evidence/metadata JSON so later rule edits do not change the explanation of an existing event.

The current detector has no stable tracking IDs. Person-stay and count-limit therefore operate at extracted-frame granularity; target tracking will refine identity and dwell precision in phase D.

## API and Persistence

Rule list and update endpoints return and accept the new optional geometry, threshold, and enabled fields. The API validates values before persisting; invalid configurations return a `400` error with a human-readable field code.

The Worker loads enabled rules rather than hard-coding `person_stay`. If a stored geometry is malformed, the Worker logs its event type and skips only that rule; it still completes the video task and evaluates all other valid rules.

## Testing and Acceptance

- Unit tests cover polygon inclusion (including boundary), rectangle serialization, intrusion candidates, dwell filtering, count thresholds, and disabled rules.
- Persistence/API tests cover rule round-trip, validation failures, and loading saved rules into a fresh application process.
- Frontend tests cover background switching, blank fallback, shape validation, and successful save payloads.

Acceptance: a user can draw a zone on either background, save one of the initial rules, restart the API, upload a video, and receive an event only when matching detections meet that rule inside the configured zone. The event detail retains the trigger geometry after the rule changes.
