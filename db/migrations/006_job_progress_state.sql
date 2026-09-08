ALTER TABLE video_jobs
  ADD COLUMN progress_stage VARCHAR(64) NULL,
  ADD COLUMN status_message TEXT NULL,
  ADD COLUMN attempt INT UNSIGNED NOT NULL DEFAULT 0,
  ADD INDEX idx_video_jobs_updated_at (updated_at);
