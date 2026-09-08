import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from './api/client';
import { applyJobProgressToJob, buildJobProgressFromJob, buildRetryProgress, jobActivityLabel, mergeJobProgress, mergeJobsSnapshot, orderJobsByPriority, progressPercent } from './features/jobProgress';
import { JobTaskDrawer } from './features/JobTaskDrawer';
import { JobsPage, ModelsPage, RulesPage } from './features/WorkspacePages';
import { detectionSummary, displayEventType, fallbackAnalysis, groupEvents, preciseTime } from './features/eventPresentation';
import type { Detection, EventItem, EventRule, JobConnectionState, JobProgressEvent, VideoJob } from './types/events';
import './styles.css';

const label = (status: EventItem['status']) => ({ unreviewed: '待复核', confirmed: '已确认', ignored: '已忽略', processing: '处理中', resolved: '已处置', closed: '已关闭' }[status]);
const mediaUrl = (source: string) => `/media/${source.replace(/\\/g, '/').replace(/^\/?media\//, '')}`;
const time = (ms: number) => `${Math.floor(ms / 60000).toString().padStart(2, '0')}:${Math.floor(ms / 1000 % 60).toString().padStart(2, '0')}`;
const box = ([left, top, right, bottom]: Detection['bbox'], width: number, height: number) => {
  const legacyPixels = [left, top, right, bottom].some(value => value > 1);
  const values = legacyPixels ? [left / width, top / height, right / width, bottom / height] : [left, top, right, bottom];
  const [x1, y1, x2, y2] = values.map(value => Math.max(0, Math.min(1, value)));
  return { left: `${x1 * 100}%`, top: `${y1 * 100}%`, width: `${Math.max(0, x2 - x1) * 100}%`, height: `${Math.max(0, y2 - y1) * 100}%` };
};

type JobPool = {
  jobsById: Record<string, VideoJob>;
  progressById: Record<string, JobProgressEvent>;
  activeJobId: string | null;
  connectionState: JobConnectionState;
};

type JobResultSummary = {
  eventCount: number;
  firstEvent: EventItem | null;
};

const initialJobPool: JobPool = {
  jobsById: {},
  progressById: {},
  activeJobId: null,
  connectionState: 'connecting',
};

function pickActiveJobId(jobsById: Record<string, VideoJob>, preferredJobId: string | null) {
  if (preferredJobId && jobsById[preferredJobId]) {
    return preferredJobId;
  }

  return Object.values(jobsById).find((item) => !['completed', 'failed', 'cancelled'].includes(item.status))?.id ?? Object.keys(jobsById)[0] ?? null;
}

function pickSelectedEvent(nextEvents: EventItem[], current: EventItem | null) {
  if (!current) {
    return nextEvents[0] ?? null;
  }

  return nextEvents.find((item) => item.id === current.id) ?? nextEvents[0] ?? null;
}

type JobSummaryBarProps = {
  job: VideoJob;
  progressEvent?: JobProgressEvent | null;
  eventCount?: number | null;
  onOpen: (jobId: string) => void;
};

function JobSummaryBar({ job, progressEvent, eventCount, onOpen }: JobSummaryBarProps) {
  const progress = typeof progressEvent?.progress === 'number'
    ? progressPercent(progressEvent.progress)
    : Math.max(0, Math.min(100, Math.round(job.progress)));
  const activity = jobActivityLabel(progressEvent, job.status);
  const status = progressEvent?.status ?? job.status;
  const summaryClass = ['completed', 'failed', 'cancelled'].includes(status) ? status : 'active';

  return (
    <button className={`job-summary-bar ${summaryClass}`} type="button" onClick={() => onOpen(job.id)} aria-label={`查看任务详情 ${job.filename}`}>
      <div className="job-summary-copy">
        <p className="eyebrow">LIVE TASK SUMMARY</p>
        <strong>{job.filename}</strong>
        <span>{activity}</span>
        {progressEvent?.message && <small>{progressEvent.message}</small>}
      </div>
      <div className="job-summary-meta">
        <strong>{`${progress}%`}</strong>
        <span>{['completed', 'failed', 'cancelled'].includes(status) ? jobActivityLabel(progressEvent, job.status) : '处理中'}</span>
        {typeof eventCount === 'number' && eventCount > 0 && <small>{`关联事件 ${eventCount} 条`}</small>}
      </div>
    </button>
  );
}

export default function App() {
  const [events, setEvents] = useState<EventItem[]>([]);
  const [selected, setSelected] = useState<EventItem | null>(null);
  const [rules, setRules] = useState<EventRule[]>([]);
  const [jobPool, setJobPool] = useState<JobPool>(initialJobPool);
  const [jobResultsById, setJobResultsById] = useState<Record<string, JobResultSummary>>({});
  const [drawerJobId, setDrawerJobId] = useState<string | null>(null);
  const [keyword, setKeyword] = useState('');
  const [statusFilter, setStatusFilter] = useState<EventItem['status'] | ''>('');
  const [severityFilter, setSeverityFilter] = useState('');
  const [page, setPage] = useState(1);
  const [activeNav, setActiveNav] = useState('事件检索');
  const [frameIndex, setFrameIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [deleting, setDeleting] = useState<EventItem | null>(null);
  const [error, setError] = useState('');
  const [feedback, setFeedback] = useState('');
  const [failedEvidenceUrls, setFailedEvidenceUrls] = useState<string[]>([]);
  const [evidenceSize, setEvidenceSize] = useState({ width: 1, height: 1 });
  const [reviewDialog, setReviewDialog] = useState<EventItem['status'] | null>(null);
  const [reviewer, setReviewer] = useState('');
  const [reviewNote, setReviewNote] = useState('');
  const [disposition, setDisposition] = useState('');
  const [reviewHistory, setReviewHistory] = useState<import('./types/events').EventReview[]>([]);
  const [playbackMode, setPlaybackMode] = useState<'original' | 'annotated'>('original');
  const pollingIntervalRef = useRef<number | null>(null);
  const pollingInFlightRef = useRef(false);
  const jobResultRequestsRef = useRef<Partial<Record<string, Promise<JobResultSummary>>>>({});

  const jobs = useMemo(() => orderJobsByPriority(jobPool.jobsById, jobPool.activeJobId), [jobPool.activeJobId, jobPool.jobsById]);
  const activeJob = useMemo(() => {
    if (jobPool.activeJobId && jobPool.jobsById[jobPool.activeJobId]) {
      return jobPool.jobsById[jobPool.activeJobId];
    }

    return jobs[0] ?? null;
  }, [jobPool.activeJobId, jobPool.jobsById, jobs]);
  const activeJobProgress = activeJob ? jobPool.progressById[activeJob.id] ?? null : null;
  const eventsByJobId = useMemo(() => events.reduce<Record<string, EventItem[]>>((groupsByJobId, item) => {
    groupsByJobId[item.job_id] = groupsByJobId[item.job_id] ? [...groupsByJobId[item.job_id], item] : [item];
    return groupsByJobId;
  }, {}), [events]);
  const activeJobResult = activeJob ? jobResultsById[activeJob.id] ?? null : null;
  const activeJobEventCount = activeJobResult?.eventCount ?? null;
  const drawerJob = drawerJobId ? jobPool.jobsById[drawerJobId] ?? null : null;
  const drawerProgress = drawerJob ? jobPool.progressById[drawerJob.id] ?? null : null;
  const drawerJobResult = drawerJob ? jobResultsById[drawerJob.id] ?? null : null;
  const drawerEventCount = drawerJobResult?.eventCount ?? null;

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.listEvents(50), api.listRules(), api.listJobs()])
      .then(([nextEvents, nextRules, nextJobs]) => {
        if (cancelled) return;
        setEvents(nextEvents);
        setSelected(current => pickSelectedEvent(nextEvents, current));
        setRules(nextRules);
        setJobPool(current => {
          const merged = mergeJobsSnapshot(current.jobsById, nextJobs, current.progressById, current.activeJobId);
          return { ...current, ...merged, activeJobId: pickActiveJobId(merged.jobsById, current.activeJobId) };
        });
      })
      .catch(cause => setError(cause instanceof Error ? cause.message : '初始化数据失败'));
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (drawerJobId && !jobPool.jobsById[drawerJobId]) {
      setDrawerJobId(null);
    }
  }, [drawerJobId, jobPool.jobsById]);

  useEffect(() => {
    if (activeJob?.status === 'completed') {
      void ensureJobResult(activeJob.id).catch(() => undefined);
    }
  }, [activeJob?.id, activeJob?.status]);

  async function refreshEvents() {
    const next = await api.listEvents(50);
    setEvents(next);
    setSelected(current => pickSelectedEvent(next, current));
  }

  async function ensureJobResult(jobId: string, options?: { force?: boolean }) {
    const cached = jobResultsById[jobId];
    if (!options?.force && cached) {
      return cached;
    }

    const requestKey = options?.force ? `${jobId}:force` : jobId;
    const inFlightRequest = jobResultRequestsRef.current[requestKey];
    if (!options?.force && inFlightRequest) {
      return inFlightRequest;
    }

    const request = api.queryEvents(new URLSearchParams({ job_id: jobId, page: '1', page_size: '1' }).toString())
      .then((page) => {
        const result = { eventCount: page.total, firstEvent: page.items[0] ?? null };
        setJobResultsById(current => ({ ...current, [jobId]: result }));
        delete jobResultRequestsRef.current[requestKey];
        return result;
      })
      .catch((cause) => {
        delete jobResultRequestsRef.current[requestKey];
        throw cause;
      });

    jobResultRequestsRef.current[requestKey] = request;
    return request;
  }

  function cacheResultEvent(nextEvent: EventItem) {
    setEvents(current => current.some((item) => item.id === nextEvent.id) ? current : [nextEvent, ...current]);
    return nextEvent;
  }

  async function refreshJobs(options?: { suppressError?: boolean }) {
    if (pollingInFlightRef.current) {
      return;
    }

    pollingInFlightRef.current = true;
    let shouldRefreshEvents = false;
    try {
      const nextJobs = await api.listJobs();
      shouldRefreshEvents = nextJobs.some((item) => ['completed', 'failed', 'cancelled'].includes(item.status) && !['completed', 'failed', 'cancelled'].includes(jobPool.jobsById[item.id]?.status ?? ''));
      setJobPool(current => {
        const merged = mergeJobsSnapshot(current.jobsById, nextJobs, current.progressById, current.activeJobId);
        return { ...current, ...merged, activeJobId: pickActiveJobId(merged.jobsById, current.activeJobId) };
      });
      if (shouldRefreshEvents) {
        await refreshEvents();
      }
    } catch (cause) {
      if (!options?.suppressError) {
        setError(cause instanceof Error ? cause.message : '任务刷新失败');
      }
      throw cause;
    } finally {
      pollingInFlightRef.current = false;
    }
  }

  useEffect(() => {
    const stopPolling = () => {
      if (pollingIntervalRef.current !== null) {
        window.clearInterval(pollingIntervalRef.current);
        pollingIntervalRef.current = null;
      }
    };
    const startPolling = () => {
      if (pollingIntervalRef.current !== null) {
        return;
      }

      pollingIntervalRef.current = window.setInterval(() => {
        void refreshJobs({ suppressError: false }).catch(() => undefined);
      }, 3000);
    };
    const unsubscribe = api.subscribeJobProgress((incoming) => {
      let reachedTerminal = false;
      const incomingTerminal = ['completed', 'failed', 'cancelled'].includes(incoming.status);
      setJobPool(current => {
        const currentProgress = current.progressById[incoming.job_id];
        const mergedProgress = currentProgress ? mergeJobProgress(currentProgress, incoming) : incoming;
        reachedTerminal = ['completed', 'failed', 'cancelled'].includes(mergedProgress.status) && currentProgress?.status !== mergedProgress.status;
        const nextProgressById = currentProgress === mergedProgress ? current.progressById : { ...current.progressById, [incoming.job_id]: mergedProgress };
        const currentJob = current.jobsById[incoming.job_id];
        const nextJobsById = currentJob ? { ...current.jobsById, [incoming.job_id]: applyJobProgressToJob(currentJob, mergedProgress) } : current.jobsById;
        return {
          ...current,
          jobsById: nextJobsById,
          progressById: nextProgressById,
          activeJobId: pickActiveJobId(nextJobsById, incoming.job_id),
        };
      });
      if (reachedTerminal || incomingTerminal) {
        void refreshEvents();
        void refreshJobs({ suppressError: true }).catch(() => undefined);
      }
    }, (state) => {
      setJobPool(current => ({ ...current, connectionState: state }));
      if (state === 'reconnecting') {
        startPolling();
        void refreshJobs({ suppressError: true }).catch(() => undefined);
      } else {
        stopPolling();
      }
    });

    return () => {
      stopPolling();
      unsubscribe();
    };
  }, []);

  const groups = useMemo(
    () => groupEvents(events.filter(event => (!keyword || `${event.event_type} ${event.analysis?.summary ?? ''}`.includes(keyword)) && (!statusFilter || event.status === statusFilter) && (!severityFilter || event.severity === severityFilter))).slice((page - 1) * 20, page * 20),
    [events, keyword, statusFilter, severityFilter, page],
  );
  const frames = selected?.evidence.frames ?? [];
  const activeFrame = frames[frameIndex] ?? frames[0];
  const analysis = selected ? selected.analysis ?? fallbackAnalysis(selected) : null;
  const selectedJob = selected ? jobPool.jobsById[selected.job_id] ?? null : null;
  const hasAnnotatedPlayback = selectedJob?.annotated_video_status === 'ready' && Boolean(selectedJob.annotated_video_url);
  const hasOriginalVideo = Boolean(selectedJob?.source_uri);
  const effectivePlaybackMode = playbackMode === 'annotated' && !hasAnnotatedPlayback ? 'original' : playbackMode;
  const connectionCopy: Record<JobConnectionState, { title: string; detail: string }> = {
    connecting: { title: '实时连接中', detail: '正在订阅任务进度流' },
    connected: { title: '系统在线', detail: 'API · WORKER · SSE' },
    reconnecting: { title: '实时重连中', detail: 'SSE 已断开，正在轮询任务状态' },
    offline: { title: '实时连接离线', detail: '请检查网络与服务状态' },
  };

  async function choose(event: EventItem) { setSelected(event); setFrameIndex(0); setPlaybackMode('original'); setFeedback(''); setFailedEvidenceUrls([]); setEvidenceSize({ width: 1, height: 1 }); setReviewHistory(await api.listReviews(event.id).catch(() => [])); }
  function upsertJob(nextJob: VideoJob, preferredJobId = nextJob.id, mode: 'preserve-progress' | 'replace-progress' = 'preserve-progress') {
    setJobPool(current => {
      const previousProgress = current.progressById[nextJob.id];
      const progress = mode === 'replace-progress' ? buildJobProgressFromJob(nextJob, previousProgress) : previousProgress;
      const mergedJob = progress ? applyJobProgressToJob(nextJob, progress) : nextJob;
      const jobsById = { ...current.jobsById, [mergedJob.id]: mergedJob };
      const progressById = mode === 'replace-progress'
        ? (() => {
            const nextProgressById = { ...current.progressById };
            if (progress) {
              nextProgressById[nextJob.id] = progress;
            } else {
              delete nextProgressById[nextJob.id];
            }
            return nextProgressById;
          })()
        : current.progressById;
      return {
        ...current,
        jobsById,
        progressById,
        activeJobId: pickActiveJobId(jobsById, preferredJobId),
      };
    });
  }
  async function upload(file?: File) {
    if (!file) return;
    setLoading(true); setError('');
    try {
      const created = await api.uploadVideo(file);
      upsertJob(created);
      if (created.status === 'pending') {
        void api.processVideo(created.id)
          .then(processed => upsertJob(processed, created.id, 'replace-progress'))
          .catch(cause => setError(cause instanceof Error ? cause.message : '视频任务处理失败'));
      }
    } catch (cause) { setError(cause instanceof Error ? cause.message : '视频任务处理失败'); }
    finally { setLoading(false); }
  }
  async function retryJob(id: string) {
    setError('');
    setJobResultsById(current => {
      if (!current[id]) {
        return current;
      }

      const next = { ...current };
      delete next[id];
      return next;
    });
    setJobPool(current => {
      const currentJob = current.jobsById[id];
      if (!currentJob) {
        return current;
      }
      const retryProgress = buildRetryProgress(id, current.progressById[id]);

      const jobsById = {
        ...current.jobsById,
        [id]: applyJobProgressToJob({ ...currentJob, status: 'pending', progress: 0 }, retryProgress),
      };
      return {
        ...current,
        jobsById,
        progressById: { ...current.progressById, [id]: retryProgress },
        activeJobId: pickActiveJobId(jobsById, id),
      };
    });

    try {
      upsertJob(await api.processVideo(id), id, 'replace-progress');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : '视频任务处理失败');
    }
  }
  function openTaskDrawer(id: string) {
    setDrawerJobId(id);
    if (jobPool.jobsById[id]?.status === 'completed') {
      void ensureJobResult(id).catch((cause) => {
        setError(cause instanceof Error ? cause.message : '任务结果加载失败');
      });
    }
  }
  function closeTaskDrawer() {
    setDrawerJobId(null);
  }
  async function resolveJobEvent(id: string) {
    const relatedEvent = eventsByJobId[id]?.[0];
    if (relatedEvent) {
      return relatedEvent;
    }

    const result = await ensureJobResult(id);
    return result.firstEvent ? cacheResultEvent(result.firstEvent) : null;
  }
  function openJob(id: string) {
    closeTaskDrawer();
    setActiveNav('事件检索');
    void resolveJobEvent(id)
      .then((relatedEvent) => {
        if (relatedEvent) {
          return choose(relatedEvent);
        }
        return undefined;
      })
      .catch((cause) => {
        setError(cause instanceof Error ? cause.message : '任务结果加载失败');
      });
  }
  function openJobPlayback(id: string) {
    closeTaskDrawer();
    setActiveNav('事件检索');
    void resolveJobEvent(id)
      .then((relatedEvent) => {
        if (!relatedEvent) {
          return;
        }

        return choose(relatedEvent).then(() => {
          setPlaybackMode('annotated');
        });
      })
      .catch((cause) => {
        setError(cause instanceof Error ? cause.message : '任务结果加载失败');
      });
  }
  async function review(action: 'confirm' | 'ignore') {
    if (!selected) return;
    setReviewing(true); setError(''); setFeedback('');
    try { const updated = action === 'confirm' ? await api.confirmEvent(selected.id) : await api.ignoreEvent(selected.id); setEvents(items => items.map(item => item.id === updated.id ? updated : item)); setSelected(updated); setFeedback(action === 'confirm' ? '事件已确认' : '事件已忽略'); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '事件状态更新失败'); }
    finally { setReviewing(false); }
  }
  async function submitReview() {
    if (!selected || !reviewDialog) return;
    setReviewing(true); setError('');
    try { const updated = await api.reviewEvent(selected.id, { status: reviewDialog, reviewer: reviewer || undefined, note: reviewNote || undefined, disposition: disposition || undefined }); setEvents(items => items.map(item => item.id === updated.id ? updated : item)); setSelected(updated); setReviewHistory(await api.listReviews(updated.id)); setReviewDialog(null); setFeedback('审核已保存'); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '审核保存失败'); }
    finally { setReviewing(false); }
  }
  async function remove() {
    if (!deleting) return;
    try { await api.deleteEvent(deleting.id); setEvents(items => items.filter(item => item.id !== deleting.id)); setSelected(current => current?.id === deleting.id ? null : current); setDeleting(null); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '删除事件失败'); }
  }

  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">V</span><div><strong>VISION OPS</strong><small>EVENT WORKSPACE</small></div></div>
      <nav>{['事件检索', '视频任务', '规则配置', '模型版本'].map(item => <button key={item} className={activeNav === item ? 'active' : ''} onClick={() => setActiveNav(item)}>{item}</button>)}</nav>
      <div className="system"><span className="dot" />{connectionCopy[jobPool.connectionState].title}<small>{connectionCopy[jobPool.connectionState].detail}</small></div>
    </aside>
    <main className="content">
      <header>
        <div><p className="eyebrow">视觉事件运营平台 / PHASE 02</p><h1>视频事件检索</h1><p className="subtitle">把视频里的异常，变成可检索、可复核的业务事件。</p></div>
        <label className="primary upload-button">{loading ? '处理中…' : '＋ 导入视频任务'}<input aria-label="导入视频任务" type="file" accept="video/*" disabled={loading} onChange={event => { upload(event.target.files?.[0]); event.currentTarget.value = ''; }} /></label>
      </header>
      {error && <div className="notice" role="alert">{error}</div>}
      {feedback && <div className="feedback" role="status">{feedback}</div>}
      {activeNav === '事件检索' && <div className="event-filters"><select aria-label="状态筛选" value={statusFilter} onChange={event => { setStatusFilter(event.target.value as EventItem['status'] | ''); setPage(1); }}><option value="">全部状态</option><option value="unreviewed">筛选：待复核</option><option value="confirmed">筛选：已确认</option><option value="ignored">筛选：已忽略</option><option value="processing">筛选：处理中</option><option value="resolved">筛选：已处置</option><option value="closed">筛选：已关闭</option></select><select aria-label="严重等级筛选" value={severityFilter} onChange={event => { setSeverityFilter(event.target.value); setPage(1); }}><option value="">全部等级</option><option value="high">筛选：高</option><option value="medium">筛选：中</option><option value="low">筛选：低</option></select><button onClick={() => { const params = new URLSearchParams(); if (statusFilter) params.set('status', statusFilter); if (severityFilter) params.set('severity', severityFilter); window.open(api.exportEvents(params.toString()), '_blank'); }}>导出 CSV</button><button disabled={page <= 1} onClick={() => setPage(value => value - 1)}>上一页</button><span>第 {page} 页</span><button disabled={groups.length < 20} onClick={() => setPage(value => value + 1)}>下一页</button></div>}
      {selected && activeNav === '事件检索' && <button className="report-link" onClick={() => window.open(api.reportEvent(selected.id), '_blank')}>打开当前事件报告</button>}
      {selected && activeNav === '事件检索' && <span className="review-shortcuts"><button onClick={() => { setReviewDialog('confirmed'); setReviewer(''); setReviewNote(''); setDisposition(''); }}>带备注确认</button><button onClick={() => { setReviewDialog('ignored'); setReviewer(''); setReviewNote(''); setDisposition(''); }}>带备注忽略</button></span>}
      {selected && reviewHistory.length > 0 && activeNav === '事件检索' && <div className="review-history"><strong>审核时间线</strong>{reviewHistory.map(item => <span key={item.id}>{item.created_at} · {label(item.old_status)} → {label(item.new_status)}{item.reviewer ? ` · ${item.reviewer}` : ''}{item.note ? ` · ${item.note}` : ''}</span>)}</div>}
      {activeNav === '事件检索' ? <>
        {activeJob && <JobSummaryBar job={activeJob} progressEvent={activeJobProgress} eventCount={activeJobEventCount} onOpen={openTaskDrawer} />}
        <section className="metrics">
          <div><span>今日事件</span><strong>{String(events.length).padStart(2, '0')}</strong><small>+12.4% vs 昨日</small></div>
          <div><span>待复核</span><strong>{String(events.filter(event => event.status === 'unreviewed').length).padStart(2, '0')}</strong><small>需要人工确认</small></div>
          <div><span>处理任务</span><strong>{activeJob ? `${activeJob.progress}%` : '00'}</strong><small>{activeJob ? `${activeJob.filename} · ${jobActivityLabel(activeJobProgress, activeJob.status)}` : '等待导入'}</small></div>
          <div><span>系统状态</span><strong className="healthy">●</strong><small>全部服务正常</small></div>
        </section>
        <section className="workspace">
          <div className="event-column">
            <div className="section-head"><div><p className="eyebrow">EVENT STREAM</p><h2>事件流</h2></div><div className="search"><input aria-label="搜索事件" placeholder="搜索事件类型…" value={keyword} onChange={event => setKeyword(event.target.value)} /></div></div>
            <div className="rule-strip">规则引擎：{rules.map(rule => `${rule.event_type} · ${rule.version}`).join(' / ') || '加载中'}</div>
            {groups.length ? <div className="event-list">{groups.map(group => {
              const event = group.events[0];
              return <button className={`event-card ${selected?.id === event.id ? 'selected' : ''}`} key={group.key} onClick={() => choose(event)}>
                {event.evidence.thumbnail_url ? <img className="event-thumb image-thumb" src={event.evidence.thumbnail_url} alt="事件证据缩略图" /> : <div className="event-thumb">◉<i>{time(event.start_time_ms)}</i></div>}
                <div className="event-copy"><div><strong>{displayEventType(event.event_type)}</strong><span className={`severity ${event.severity}`}>{event.severity}</span></div><p>{(event.analysis ?? fallbackAnalysis(event)).summary}</p><small>{detectionSummary(event)} · {event.rule_version ?? 'rule-v1'}{group.events.length > 1 ? ` · ${group.events.length} 个片段` : ''}</small><em>{event.event_type}</em></div>
              </button>;
            })}</div> : <div className="empty">暂无匹配事件<br /><small>{keyword ? '尝试其他关键词' : '导入视频后，事件会出现在这里'}</small></div>}
          </div>
          <div className="detail-column">{selected ? <>
            <div className="detail-top"><div><p className="eyebrow">EVENT DETAIL / {selected.id}</p><h2>事件详情</h2></div><span className="status">{label(selected.status)}</span></div>
            {selectedJob && <div className="playback-panel"><div className="playback-heading"><div><p className="eyebrow">VIDEO PLAYBACK</p><strong>视频回放</strong></div><div className="playback-tabs">{hasOriginalVideo && <button className={effectivePlaybackMode === 'original' ? 'active' : ''} onClick={() => setPlaybackMode('original')}>原始视频</button>}{hasAnnotatedPlayback && <button className={effectivePlaybackMode === 'annotated' ? 'active' : ''} onClick={() => setPlaybackMode('annotated')}>YOLO 检测回放</button>}</div></div>{selectedJob.annotated_video_status === 'pending' && <div className="playback-status">检测回放生成中，请稍候…</div>}{selectedJob.annotated_video_status === 'failed' && <div className="playback-status playback-error">检测回放生成失败：{selectedJob.annotated_video_error || '未知原因'}</div>}{effectivePlaybackMode === 'annotated' && hasAnnotatedPlayback ? <video key={`annotated-${selectedJob.id}`} className="playback-video" controls preload="metadata" aria-label="YOLO 检测回放"><source data-testid="playback-source" src={selectedJob.annotated_video_url ?? ''} type="video/mp4" /></video> : effectivePlaybackMode === 'original' && hasOriginalVideo ? <video key={`original-${selectedJob.id}`} className="playback-video" controls preload="metadata" aria-label="原始视频"><source data-testid="playback-source" src={mediaUrl(selectedJob.source_uri ?? '')} type="video/mp4" /></video> : !['pending', 'failed'].includes(selectedJob.annotated_video_status ?? '') && <div className="playback-status">暂无可用视频回放</div>}</div>}
            <div className="evidence">{activeFrame ? <><div className="evidence-stage">{failedEvidenceUrls.includes(activeFrame.image_url) ? <div className="evidence-unavailable"><strong>证据文件不可用</strong><button aria-label="重新加载证据图片" onClick={() => setFailedEvidenceUrls(urls => urls.filter(url => url !== activeFrame.image_url))}>重新加载</button></div> : <><img src={activeFrame.image_url} alt={`${preciseTime(activeFrame.timestamp_ms)} 的检测证据`} onLoad={event => setEvidenceSize({ width: event.currentTarget.naturalWidth || 1, height: event.currentTarget.naturalHeight || 1 })} onError={() => setFailedEvidenceUrls(urls => urls.includes(activeFrame.image_url) ? urls : [...urls, activeFrame.image_url])} />{activeFrame.detections.map((detection, index) => <span className="detection-box" key={index} style={box(detection.bbox, evidenceSize.width, evidenceSize.height)}>{detection.class_name} {(detection.confidence * 100).toFixed(0)}%</span>)}</>}</div><div className="timeline">{frames.map((item, index) => <button key={item.image_url} aria-label={`证据帧 ${preciseTime(item.timestamp_ms)}`} aria-pressed={index === frameIndex} onClick={() => { setFrameIndex(index); setFailedEvidenceUrls([]); }}>{preciseTime(item.timestamp_ms)}</button>)}</div></> : <div className="evidence-empty">暂无可用抽帧证据</div>}</div>
            <div className="detail-info"><div><span>事件类型</span><strong>{displayEventType(selected.event_type)}</strong><small>{selected.event_type} · {selected.rule_version ?? 'rule-v1'}</small></div><div><span>严重等级</span><strong className="danger">{selected.severity.toUpperCase()}</strong></div><div><span>检测摘要</span><strong>{detectionSummary(selected)}</strong></div><div><span>模型置信度</span><strong>{(selected.confidence * 100).toFixed(1)}%</strong></div></div>
            <div className="analysis"><p className="eyebrow">MODEL ANALYSIS</p><h3>{analysis?.summary}</h3><p>{analysis?.suggestion}</p><small>来源：{analysis?.report_source} · 检测器：{selected.detector_version}</small></div>
            <div className="actions"><button className="confirm" onClick={() => review('confirm')} disabled={reviewing || selected.status === 'confirmed'}>{reviewing ? '保存中…' : '确认事件'}</button><button onClick={() => review('ignore')} disabled={reviewing || selected.status === 'ignored'}>{selected.status === 'ignored' ? '已忽略' : '忽略'}</button><button className="danger-button" aria-label={`删除事件 ${selected.event_type}`} onClick={() => setDeleting(selected)}>删除事件</button></div>
          </> : <div className="empty detail-empty">选择一个事件查看证据与分析</div>}</div>
        </section>
      </> : activeNav === '视频任务' ? <JobsPage jobs={jobs} progressById={jobPool.progressById} connectionState={jobPool.connectionState} onOpenJob={openJob} onRetryJob={retryJob} onRefresh={async () => refreshJobs({ suppressError: false })} /> : activeNav === '规则配置' ? <RulesPage rules={rules} events={events} onSaved={async () => setRules(await api.listRules())} /> : <ModelsPage />}
      {drawerJob && <JobTaskDrawer job={drawerJob} progressEvent={drawerProgress} eventCount={drawerEventCount} onClose={closeTaskDrawer} onRetry={retryJob} onViewEvents={drawerJobResult?.firstEvent ? openJob : undefined} onViewPlayback={drawerJobResult?.firstEvent ? openJobPlayback : undefined} />}
      {deleting && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true" aria-labelledby="delete-event-title"><h3 id="delete-event-title">确认删除事件？</h3><p>事件“{deleting.event_type}”将被永久删除，原视频不会受到影响。</p><div className="modal-actions"><button onClick={() => setDeleting(null)}>取消</button><button className="danger-button" onClick={remove}>确认删除</button></div></div></div>}
      {reviewDialog && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true"><h3>{reviewDialog === 'confirmed' ? '确认事件' : '忽略事件'}</h3><label>审核人<input value={reviewer} onChange={event => setReviewer(event.target.value)} placeholder="可选" /></label><label>处置结果<input value={disposition} onChange={event => setDisposition(event.target.value)} placeholder="例如：通知现场人员" /></label><label>备注<textarea value={reviewNote} onChange={event => setReviewNote(event.target.value)} placeholder="填写审核说明" /></label><div className="modal-actions"><button onClick={() => setReviewDialog(null)}>取消</button><button className="confirm" onClick={submitReview} disabled={reviewing}>{reviewing ? '保存中…' : '提交审核'}</button></div></div></div>}
    </main>
  </div>;
}
