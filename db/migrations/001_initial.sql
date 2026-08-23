CREATE TABLE IF NOT EXISTS video_jobs (
  id CHAR(36) PRIMARY KEY,
  filename VARCHAR(255) NOT NULL,
  duration_ms BIGINT UNSIGNED NOT NULL DEFAULT 0,
  status VARCHAR(32) NOT NULL,
  progress TINYINT UNSIGNED NOT NULL DEFAULT 0,
  source_uri TEXT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  INDEX idx_video_jobs_status (status),
  INDEX idx_video_jobs_created_at (created_at)
);

CREATE TABLE IF NOT EXISTS events (
  id CHAR(36) PRIMARY KEY,
  job_id CHAR(36) NOT NULL,
  event_type VARCHAR(128) NOT NULL,
  start_time_ms BIGINT UNSIGNED NOT NULL,
  end_time_ms BIGINT UNSIGNED NOT NULL,
  severity VARCHAR(32) NOT NULL,
  status VARCHAR(32) NOT NULL,
  confidence FLOAT NOT NULL,
  objects_json JSON NOT NULL,
  evidence_json JSON NOT NULL,
  analysis_json JSON NULL,
  rule_version VARCHAR(64) NOT NULL,
  prompt_version VARCHAR(64) NULL,
  detector_version VARCHAR(64) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  INDEX idx_events_type_time (event_type, start_time_ms),
  INDEX idx_events_status (status),
  CONSTRAINT fk_events_job FOREIGN KEY (job_id) REFERENCES video_jobs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS event_rules (
  event_type VARCHAR(128) PRIMARY KEY,
  class_name VARCHAR(128) NOT NULL,
  min_confidence FLOAT NOT NULL,
  min_duration_ms BIGINT UNSIGNED NOT NULL,
  version VARCHAR(64) NOT NULL,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

INSERT INTO event_rules (event_type, class_name, min_confidence, min_duration_ms, version)
VALUES ('person_stay', 'person', 0.8, 1000, 'rule-v1')
ON DUPLICATE KEY UPDATE event_type = event_type;

