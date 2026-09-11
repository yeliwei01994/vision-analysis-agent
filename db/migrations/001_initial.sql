CREATE TABLE IF NOT EXISTS video_jobs (
  id UUID PRIMARY KEY,
  filename VARCHAR(255) NOT NULL,
  duration_ms BIGINT NOT NULL DEFAULT 0,
  status VARCHAR(32) NOT NULL,
  progress SMALLINT NOT NULL DEFAULT 0,
  source_uri TEXT,
  deleted_at TIMESTAMPTZ,
  annotated_video_url TEXT,
  annotated_video_status VARCHAR(32),
  annotated_video_error TEXT,
  progress_stage VARCHAR(64),
  status_message TEXT,
  attempt INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_video_jobs_status ON video_jobs (status);
CREATE INDEX idx_video_jobs_created_at ON video_jobs (created_at);
CREATE INDEX idx_video_jobs_deleted_at ON video_jobs (deleted_at);
CREATE INDEX idx_video_jobs_updated_at ON video_jobs (updated_at);

CREATE TABLE IF NOT EXISTS events (
  id UUID PRIMARY KEY,
  job_id UUID NOT NULL REFERENCES video_jobs(id) ON DELETE CASCADE,
  event_type VARCHAR(128) NOT NULL,
  start_time_ms BIGINT NOT NULL,
  end_time_ms BIGINT NOT NULL,
  severity VARCHAR(32) NOT NULL,
  status VARCHAR(32) NOT NULL,
  confidence REAL NOT NULL,
  objects_json JSONB NOT NULL,
  evidence_json JSONB NOT NULL,
  analysis_json JSONB,
  rule_version VARCHAR(64) NOT NULL,
  prompt_version VARCHAR(64),
  detector_version VARCHAR(64) NOT NULL,
  reviewer VARCHAR(128),
  reviewed_at TIMESTAMPTZ,
  review_note TEXT,
  disposition VARCHAR(128),
  zone_key VARCHAR(128),
  association_key VARCHAR(255),
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_events_type_time ON events (event_type, start_time_ms);
CREATE INDEX idx_events_status ON events (status);
CREATE INDEX idx_events_job_time ON events (job_id, start_time_ms);
CREATE INDEX idx_events_zone_status_time ON events (zone_key, status, start_time_ms);
CREATE INDEX idx_events_association ON events (association_key);
CREATE INDEX idx_events_objects_json_gin ON events USING GIN (objects_json);

CREATE TABLE IF NOT EXISTS event_rules (
  event_type VARCHAR(128) PRIMARY KEY,
  class_name VARCHAR(128) NOT NULL,
  min_confidence REAL NOT NULL,
  min_duration_ms BIGINT NOT NULL,
  version VARCHAR(64) NOT NULL,
  geometry_json JSONB,
  threshold_value INTEGER,
  enabled BOOLEAN NOT NULL DEFAULT TRUE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_event_rules_geometry_json_gin ON event_rules USING GIN (geometry_json);

CREATE TABLE IF NOT EXISTS event_review_history (
  id UUID PRIMARY KEY,
  event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  old_status VARCHAR(32) NOT NULL,
  new_status VARCHAR(32) NOT NULL,
  reviewer VARCHAR(128),
  note TEXT,
  disposition VARCHAR(128),
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_event_review_history_event_time ON event_review_history (event_id, created_at);

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = CURRENT_TIMESTAMP;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER video_jobs_set_updated_at
BEFORE UPDATE ON video_jobs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER event_rules_set_updated_at
BEFORE UPDATE ON event_rules
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

INSERT INTO event_rules (event_type, class_name, min_confidence, min_duration_ms, version, threshold_value, enabled)
VALUES
  ('person_stay', 'person', 0.8, 1000, 'rule-v1', NULL, TRUE),
  ('person_enter_zone', 'person', 0.25, 0, 'rule-v1', NULL, TRUE),
  ('person_count_limit', 'person', 0.25, 0, 'rule-v1', 1, TRUE)
ON CONFLICT (event_type) DO NOTHING;
