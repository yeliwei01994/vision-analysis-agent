import { useEffect, useMemo, useState } from 'react';
import { api } from './api/client';
import { JobsPage, ModelsPage, RulesPage } from './features/WorkspacePages';
import { detectionSummary, displayEventType, fallbackAnalysis, groupEvents, preciseTime } from './features/eventPresentation';
import type { Detection, EventItem, EventRule, VideoJob } from './types/events';
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

export default function App() {
  const [events, setEvents] = useState<EventItem[]>([]);
  const [selected, setSelected] = useState<EventItem | null>(null);
  const [rules, setRules] = useState<EventRule[]>([]);
  const [jobs, setJobs] = useState<VideoJob[]>([]);
  const [job, setJob] = useState<VideoJob | null>(null);
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
  const [uploadName, setUploadName] = useState('');
  const [failedEvidenceUrls, setFailedEvidenceUrls] = useState<string[]>([]);
  const [evidenceSize, setEvidenceSize] = useState({ width: 1, height: 1 });
  const [reviewDialog, setReviewDialog] = useState<EventItem['status'] | null>(null);
  const [reviewer, setReviewer] = useState('');
  const [reviewNote, setReviewNote] = useState('');
  const [disposition, setDisposition] = useState('');
  const [reviewHistory, setReviewHistory] = useState<import('./types/events').EventReview[]>([]);
  const [playbackMode, setPlaybackMode] = useState<'original' | 'annotated'>('original');

  useEffect(() => {
    Promise.all([api.listEvents(), api.listRules(), api.listJobs()])
      .then(([nextEvents, nextRules, nextJobs]) => {
        setEvents(nextEvents); setSelected(nextEvents[0] ?? null); setRules(nextRules); setJobs(nextJobs);
      })
      .catch(cause => setError(cause instanceof Error ? cause.message : '初始化数据失败'));
  }, []);

  const groups = useMemo(
    () => groupEvents(events.filter(event => (!keyword || `${event.event_type} ${event.analysis?.summary ?? ''}`.includes(keyword)) && (!statusFilter || event.status === statusFilter) && (!severityFilter || event.severity === severityFilter))).slice((page - 1) * 20, page * 20),
    [events, keyword, statusFilter, severityFilter, page],
  );
  const frames = selected?.evidence.frames ?? [];
  const activeFrame = frames[frameIndex] ?? frames[0];
  const analysis = selected ? selected.analysis ?? fallbackAnalysis(selected) : null;
  const selectedJob = selected ? jobs.find(item => item.id === selected.job_id) : null;
  const hasAnnotatedPlayback = selectedJob?.annotated_video_status === 'ready' && Boolean(selectedJob.annotated_video_url);
  const hasOriginalVideo = Boolean(selectedJob?.source_uri);
  const effectivePlaybackMode = playbackMode === 'annotated' && !hasAnnotatedPlayback ? 'original' : playbackMode;

  async function choose(event: EventItem) { setSelected(event); setFrameIndex(0); setPlaybackMode('original'); setFeedback(''); setFailedEvidenceUrls([]); setEvidenceSize({ width: 1, height: 1 }); setReviewHistory(await api.listReviews(event.id).catch(() => [])); }
  async function refreshEvents() { const next = await api.listEvents(); setEvents(next); setSelected(next[0] ?? null); }
  async function waitForJob(id: string) {
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const current = await api.getJob(id); setJob(current);
      if (['completed', 'failed', 'cancelled'].includes(current.status)) return current;
      await new Promise(resolve => window.setTimeout(resolve, 1000));
    }
    throw new Error('任务处理超时，请检查 Worker 日志');
  }
  async function upload(file?: File) {
    if (!file) return;
    setLoading(true); setError(''); setUploadName(file.name);
    try {
      const created = await api.uploadVideo(file); setJob(created);
      if (created.status === 'pending') await api.processVideo(created.id);
      const finished = await waitForJob(created.id);
      if (finished.status === 'failed') throw new Error('视频处理失败，请检查 Worker 日志');
      await refreshEvents(); setJobs(await api.listJobs());
    } catch (cause) { setError(cause instanceof Error ? cause.message : '视频任务处理失败'); }
    finally { setLoading(false); }
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
      <div className="system"><span className="dot" />系统在线<small>API · WORKER · STORAGE</small></div>
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
        <section className="metrics">
          <div><span>今日事件</span><strong>{String(events.length).padStart(2, '0')}</strong><small>+12.4% vs 昨日</small></div>
          <div><span>待复核</span><strong>{String(events.filter(event => event.status === 'unreviewed').length).padStart(2, '0')}</strong><small>需要人工确认</small></div>
          <div><span>处理任务</span><strong>{job ? `${job.progress}%` : '00'}</strong><small>{uploadName || job?.status || '等待导入'}</small></div>
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
            {selectedJob && <div className="playback-panel"><div className="playback-heading"><div><p className="eyebrow">VIDEO PLAYBACK</p><strong>视频回放</strong></div><div className="playback-tabs">{hasOriginalVideo && <button className={effectivePlaybackMode === 'original' ? 'active' : ''} onClick={() => setPlaybackMode('original')}>原始视频</button>}{hasAnnotatedPlayback && <button className={effectivePlaybackMode === 'annotated' ? 'active' : ''} onClick={() => setPlaybackMode('annotated')}>YOLO 检测回放</button>}</div></div>{selectedJob.annotated_video_status === 'pending' && <div className="playback-status">检测回放生成中，请稍候…</div>}{selectedJob.annotated_video_status === 'failed' && <div className="playback-status playback-error">检测回放生成失败：{selectedJob.annotated_video_error || '未知原因'}</div>}{effectivePlaybackMode === 'annotated' && hasAnnotatedPlayback ? <video className="playback-video" controls preload="metadata" aria-label="YOLO 检测回放"><source data-testid="playback-source" src={selectedJob.annotated_video_url ?? ''} type="video/mp4" /></video> : effectivePlaybackMode === 'original' && hasOriginalVideo ? <video className="playback-video" controls preload="metadata" aria-label="原始视频"><source data-testid="playback-source" src={mediaUrl(selectedJob.source_uri ?? '')} type="video/mp4" /></video> : !['pending', 'failed'].includes(selectedJob.annotated_video_status ?? '') && <div className="playback-status">暂无可用视频回放</div>}</div>}
            <div className="evidence">{activeFrame ? <><div className="evidence-stage">{failedEvidenceUrls.includes(activeFrame.image_url) ? <div className="evidence-unavailable"><strong>证据文件不可用</strong><button aria-label="重新加载证据图片" onClick={() => setFailedEvidenceUrls(urls => urls.filter(url => url !== activeFrame.image_url))}>重新加载</button></div> : <><img src={activeFrame.image_url} alt={`${preciseTime(activeFrame.timestamp_ms)} 的检测证据`} onLoad={event => setEvidenceSize({ width: event.currentTarget.naturalWidth || 1, height: event.currentTarget.naturalHeight || 1 })} onError={() => setFailedEvidenceUrls(urls => urls.includes(activeFrame.image_url) ? urls : [...urls, activeFrame.image_url])} />{activeFrame.detections.map((detection, index) => <span className="detection-box" key={index} style={box(detection.bbox, evidenceSize.width, evidenceSize.height)}>{detection.class_name} {(detection.confidence * 100).toFixed(0)}%</span>)}</>}</div><div className="timeline">{frames.map((item, index) => <button key={item.image_url} aria-label={`证据帧 ${preciseTime(item.timestamp_ms)}`} aria-pressed={index === frameIndex} onClick={() => { setFrameIndex(index); setFailedEvidenceUrls([]); }}>{preciseTime(item.timestamp_ms)}</button>)}</div></> : <div className="evidence-empty">暂无可用抽帧证据</div>}</div>
            <div className="detail-info"><div><span>事件类型</span><strong>{displayEventType(selected.event_type)}</strong><small>{selected.event_type} · {selected.rule_version ?? 'rule-v1'}</small></div><div><span>严重等级</span><strong className="danger">{selected.severity.toUpperCase()}</strong></div><div><span>检测摘要</span><strong>{detectionSummary(selected)}</strong></div><div><span>模型置信度</span><strong>{(selected.confidence * 100).toFixed(1)}%</strong></div></div>
            <div className="analysis"><p className="eyebrow">MODEL ANALYSIS</p><h3>{analysis?.summary}</h3><p>{analysis?.suggestion}</p><small>来源：{analysis?.report_source} · 检测器：{selected.detector_version}</small></div>
            <div className="actions"><button className="confirm" onClick={() => review('confirm')} disabled={reviewing || selected.status === 'confirmed'}>{reviewing ? '保存中…' : '确认事件'}</button><button onClick={() => review('ignore')} disabled={reviewing || selected.status === 'ignored'}>{selected.status === 'ignored' ? '已忽略' : '忽略'}</button><button className="danger-button" aria-label={`删除事件 ${selected.event_type}`} onClick={() => setDeleting(selected)}>删除事件</button></div>
          </> : <div className="empty detail-empty">选择一个事件查看证据与分析</div>}</div>
        </section>
      </> : activeNav === '视频任务' ? <JobsPage jobs={jobs} onRefresh={async () => setJobs(await api.listJobs())} /> : activeNav === '规则配置' ? <RulesPage rules={rules} events={events} onSaved={async () => setRules(await api.listRules())} /> : <ModelsPage />}
      {deleting && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true" aria-labelledby="delete-event-title"><h3 id="delete-event-title">确认删除事件？</h3><p>事件“{deleting.event_type}”将被永久删除，原视频不会受到影响。</p><div className="modal-actions"><button onClick={() => setDeleting(null)}>取消</button><button className="danger-button" onClick={remove}>确认删除</button></div></div></div>}
      {reviewDialog && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true"><h3>{reviewDialog === 'confirmed' ? '确认事件' : '忽略事件'}</h3><label>审核人<input value={reviewer} onChange={event => setReviewer(event.target.value)} placeholder="可选" /></label><label>处置结果<input value={disposition} onChange={event => setDisposition(event.target.value)} placeholder="例如：通知现场人员" /></label><label>备注<textarea value={reviewNote} onChange={event => setReviewNote(event.target.value)} placeholder="填写审核说明" /></label><div className="modal-actions"><button onClick={() => setReviewDialog(null)}>取消</button><button className="confirm" onClick={submitReview} disabled={reviewing}>{reviewing ? '保存中…' : '提交审核'}</button></div></div></div>}
    </main>
  </div>;
}
