ALTER TABLE video_jobs
  ADD COLUMN deleted_at DATETIME NULL,
  ADD INDEX idx_video_jobs_deleted_at (deleted_at);
