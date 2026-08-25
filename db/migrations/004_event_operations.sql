ALTER TABLE events
  ADD COLUMN reviewer VARCHAR(128) NULL,
  ADD COLUMN reviewed_at VARCHAR(64) NULL,
  ADD COLUMN review_note TEXT NULL,
  ADD COLUMN disposition VARCHAR(128) NULL,
  ADD COLUMN zone_key VARCHAR(128) NULL,
  ADD COLUMN association_key VARCHAR(255) NULL;

ALTER TABLE events ADD INDEX idx_events_job_time (job_id, start_time_ms);
ALTER TABLE events ADD INDEX idx_events_zone_status_time (zone_key, status, start_time_ms);
ALTER TABLE events ADD INDEX idx_events_association (association_key);

CREATE TABLE IF NOT EXISTS event_review_history (
  id CHAR(36) PRIMARY KEY,
  event_id CHAR(36) NOT NULL,
  old_status VARCHAR(32) NOT NULL,
  new_status VARCHAR(32) NOT NULL,
  reviewer VARCHAR(128) NULL,
  note TEXT NULL,
  disposition VARCHAR(128) NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  INDEX idx_event_review_history_event_time (event_id, created_at),
  CONSTRAINT fk_event_review_history_event FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
);
