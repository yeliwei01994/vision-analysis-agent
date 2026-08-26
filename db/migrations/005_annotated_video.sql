ALTER TABLE video_jobs
  ADD COLUMN annotated_video_url TEXT NULL,
  ADD COLUMN annotated_video_status VARCHAR(32) NULL,
  ADD COLUMN annotated_video_error TEXT NULL;
