# Event Evidence Experience Design

## Goal

Turn raw YOLO detections into understandable, reviewable video events. Every event must expose real frame evidence, a readable Chinese summary, a browsable detection timeline, grouped related occurrences, and clear review feedback.

## Scope

This design implements all seven requested improvements:

1. Replace repeated class names with a count and average-confidence summary.
2. Translate known event rules for display while retaining the rule version.
3. Persist real selected frames and their detection boxes for each event.
4. Render an interactive frame timeline covering the event time range.
5. Group events from the same video and rule in the event stream.
6. Generate deterministic analysis when no external LLM analysis is available.
7. Show review success/failure feedback and a status transition.

## Architecture

The worker will preserve only sampled frames that contribute to a rule event. Evidence lives in the local media root at `media/evidence/<event-id>/`; MySQL continues to store one JSON evidence document per event. The API serves the media root at `/media`, so URLs stored in evidence are usable directly by the browser.

`Evidence` becomes a list of `EvidenceFrame` items. Each item has its video timestamp, a `/media/evidence/...` image URL, and the detections observed in that image. `thumbnail_url` remains the first evidence frame for compact cards. Existing events with the old evidence JSON remain valid and render a no-evidence state rather than failing.

The rule engine must preserve the source frame information needed by an event rather than retaining only flattened `Detection` objects. Event construction maps the matching frame detections back to retained image files. The worker copies images before its temporary frame directory is removed, then persists the completed `Event` as it does today.

## Evidence Data Model

```rust
pub struct EvidenceFrame {
    pub timestamp_ms: u64,
    pub image_url: String,
    pub detections: Vec<Detection>,
}

pub struct Evidence {
    pub thumbnail_url: Option<String>,
    pub clip_url: Option<String>,
    pub frame_urls: Vec<String>,
    pub frames: Vec<EvidenceFrame>,
}
```

`frame_urls` is retained temporarily for JSON backward compatibility and is populated from `frames`. New frontend code consumes `frames`. A frame may contain several target detections. The exact `bbox` values returned by YOLO are drawn as percentage-based overlay rectangles; no fabricated geometry is used.

## API and Media Serving

The Axum router exposes `GET /media/*path`, constrained to the configured media root. The handler rejects traversal outside that root and only returns evidence images that exist. `MEDIA_ROOT` continues to control uploads and evidence storage; default is `media`.

No endpoint shape changes are required for the existing event list or event detail endpoint: the serialized `evidence.frames` field supplies the new UI. Review endpoints retain their current status-update response and errors.

## Event Stream Experience

Events are grouped client-side by `job_id + event_type + rule_version`. A group card shows the first event's thumbnail, Chinese rule label, source rule version, number of occurrences, time range, and an aggregate detection summary such as `人员 11 次检测 · 平均置信度 34%`. Selecting a group selects its first event and displays a compact occurrence selector when the group contains multiple events.

Known COCO labels and rule names use a small display dictionary, including `person → 人员` and `person_stay → 人员停留`. Unknown values safely fall back to their source string. This dictionary is presentation-only and never changes persisted rule identifiers.

## Evidence Detail and Timeline

The detail panel displays the selected evidence image or an explicit "暂无可用抽帧证据" state for old events. Each detection in the active frame is overlaid with a class label and confidence. A horizontal, keyboard-accessible timeline lists evidence frames in timestamp order. Selecting a frame updates the image, boxes, and exact timestamp; the range heading uses `00:00.5 → 00:06.0` formatting.

There is no fake video seek operation in this phase. Clicking a point selects the corresponding stored evidence image. This keeps the behavior truthful until an actual browser-playable video endpoint is added.

## Analysis, Review Feedback, and Motion

When `analysis` is absent, the frontend derives an analysis summary from event start/end time, translated target name, detection count, and average confidence. The detail panel will always show a meaningful summary and a rule-based recommendation.

Confirm and ignore show a non-blocking success notice after the API returns. API failures keep the selected event unchanged and show the server status code/message in an alert. Status badges and action buttons use a short CSS transition; the result is visible without requiring a page refresh.

## Error Handling and Compatibility

- Failure to copy evidence causes the job to continue and emits an event with empty evidence; it must be logged clearly.
- Missing media files render the no-evidence state rather than a broken image.
- Old MySQL rows deserialize because `frames` defaults to an empty list.
- Empty object lists display `未检测到目标` and never create malformed summaries.
- Grouping and summaries happen in a pure frontend utility, with unit tests for empty, repeated, and mixed labels.

## Tests

- Rust unit/contract tests prove that a persisted event serializes/deserializes `EvidenceFrame` values and worker evidence files are copied under the media root.
- Router tests prove an evidence image can be fetched from `/media` and traversal is rejected.
- Frontend tests prove Chinese summaries, fallback analysis, timeline selection, grouped occurrences, missing-evidence rendering, and review success/failure feedback.
- Existing delete and review tests remain green.

## Non-goals

- Storing image binary data in MySQL.
- Adding object storage, CDN, authentication, or LLM generation.
- Browser playback/seek synchronization with the original uploaded video.
