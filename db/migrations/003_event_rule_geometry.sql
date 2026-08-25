ALTER TABLE event_rules
  ADD COLUMN geometry_json JSON NULL,
  ADD COLUMN threshold_value INT UNSIGNED NULL,
  ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT TRUE;

INSERT INTO event_rules (event_type, class_name, min_confidence, min_duration_ms, version, threshold_value, enabled)
VALUES
  ('person_enter_zone', 'person', 0.25, 0, 'rule-v1', NULL, TRUE),
  ('person_count_limit', 'person', 0.25, 0, 'rule-v1', 1, TRUE)
ON DUPLICATE KEY UPDATE event_type = event_type;
