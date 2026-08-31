type JobProgressBarProps = {
  progress?: number;
  indeterminate?: boolean;
  label: string;
};

function clampProgress(progress?: number) {
  if (!Number.isFinite(progress)) {
    return 0;
  }

  return Math.max(0, Math.min(100, Math.round(progress ?? 0)));
}

export function JobProgressBar({ progress = 0, indeterminate = false, label }: JobProgressBarProps) {
  const value = clampProgress(progress);

  return (
    <div
      className={`job-progress-bar${indeterminate ? ' indeterminate' : ''}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      {...(indeterminate ? {} : { 'aria-valuenow': value })}
    >
      <span className="job-progress-fill" style={indeterminate ? undefined : { width: `${value}%` }} />
    </div>
  );
}
