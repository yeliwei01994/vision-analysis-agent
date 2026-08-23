import { useEffect, useMemo, useState } from 'react';
import { api } from './api/client';
import type { EventItem, EventRule, VideoJob } from './types/events';
import './styles.css';

function formatTime(ms: number) { return `${Math.floor(ms / 1000 / 60).toString().padStart(2, '0')}:${Math.floor(ms / 1000 % 60).toString().padStart(2, '0')}`; }
function statusLabel(status: EventItem['status']) { return status === 'confirmed' ? '已确认' : status === 'ignored' ? '已忽略' : '待复核'; }

export default function App() {
  const [events, setEvents] = useState<EventItem[]>([]);
  const [selected, setSelected] = useState<EventItem | null>(null);
  const [keyword, setKeyword] = useState('');
  const [job, setJob] = useState<VideoJob | null>(null);
  const [loading, setLoading] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [error, setError] = useState('');
  const [uploadName, setUploadName] = useState('');
  const [rules, setRules] = useState<EventRule[]>([]);
  const [activeNav, setActiveNav] = useState('事件检索');

  async function refreshEvents() {
    const nextEvents = await api.listEvents();
    setEvents(nextEvents);
    setSelected((current) => current && nextEvents.some((event) => event.id === current.id) ? nextEvents.find((event) => event.id === current.id) ?? null : nextEvents[0] ?? null);
  }

  useEffect(() => { Promise.all([api.listEvents(), api.listRules()]).then(([nextEvents, nextRules]) => { setEvents(nextEvents); setSelected(nextEvents[0] ?? null); setRules(nextRules); }).catch((cause) => setError(cause instanceof Error ? cause.message : '初始化数据失败')); }, []);
  const visibleEvents = useMemo(() => keyword.trim() ? events.filter((event) => `${event.event_type} ${event.analysis?.summary ?? ''}`.includes(keyword.trim())) : events, [events, keyword]);

  async function waitForJob(id: string) {
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const current = await api.getJob(id); setJob(current);
      if (current.status === 'completed' || current.status === 'failed' || current.status === 'cancelled') return current;
      await new Promise((resolve) => window.setTimeout(resolve, 1000));
    }
    throw new Error('任务处理超时，请检查 Worker 日志');
  }

  async function handleVideo(file?: File) {
    if (!file) return;
    setLoading(true); setError('');
    setUploadName(file.name);
    try { const nextJob = await api.uploadVideo(file); setJob(nextJob); await api.processVideo(nextJob.id); const finished = await waitForJob(nextJob.id); if (finished.status === 'failed') throw new Error('视频处理失败，请检查 Worker 日志'); await refreshEvents(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '视频任务处理失败'); }
    finally { setLoading(false); }
  }

  async function reviewEvent(action: 'confirm' | 'ignore') {
    if (!selected) return;
    setReviewing(true); setError('');
    try { const updated = action === 'confirm' ? await api.confirmEvent(selected.id) : await api.ignoreEvent(selected.id); setEvents((current) => current.map((event) => event.id === updated.id ? updated : event)); setSelected(updated); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '事件状态更新失败'); }
    finally { setReviewing(false); }
  }

  return <div className="shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark">V</span><div><strong>VISION OPS</strong><small>EVENT WORKSPACE</small></div></div><nav>{['事件检索', '视频任务', '规则配置', '模型版本'].map((item) => <button key={item} className={activeNav === item ? 'active' : ''} onClick={() => setActiveNav(item)}>{item}</button>)}</nav><div className="system"><span className="dot" />系统在线<small>API · WORKER · STORAGE</small></div></aside>
    <main className="content"><header><div><p className="eyebrow">视觉事件运营平台 / PHASE 02</p><h1>视频事件检索</h1><p className="subtitle">把视频里的异常，变成可检索、可复核的业务事件。</p></div><label className="primary upload-button">{loading ? '处理中…' : '＋ 导入视频任务'}<input type="file" accept="video/*" onChange={(event) => { handleVideo(event.target.files?.[0]); event.currentTarget.value = ''; }} disabled={loading} /></label></header>
      {error && <div className="notice" role="alert">{error}</div>}
      <section className="metrics"><div><span>今日事件</span><strong>{events.length.toString().padStart(2, '0')}</strong><small>+12.4% vs 昨日</small></div><div><span>待复核</span><strong>{events.filter((event) => event.status === 'unreviewed').length.toString().padStart(2, '0')}</strong><small>需要人工确认</small></div><div><span>处理任务</span><strong>{job ? `${job.progress}%` : '00'}</strong><small>{uploadName || job?.status || '等待导入'}</small></div><div><span>系统状态</span><strong className="healthy">●</strong><small>全部服务正常</small></div></section>
      <section className="workspace"><div className="event-column"><div className="section-head"><div><p className="eyebrow">EVENT STREAM</p><h2>事件流</h2></div><div className="search"><span>⌕</span><input aria-label="搜索事件" placeholder="搜索事件类型…" value={keyword} onChange={(event) => setKeyword(event.target.value)} /></div></div><div className="rule-strip">规则引擎：{rules.map((rule) => `${rule.event_type} · ${rule.version}`).join(' / ') || '加载中'}</div>{visibleEvents.length === 0 ? <div className="empty">暂无匹配事件<br /><small>{keyword ? '尝试其他关键词' : '导入视频后，事件会出现在这里'}</small></div> : <div className="event-list">{visibleEvents.map((event) => <button className={`event-card ${selected?.id === event.id ? 'selected' : ''}`} key={event.id} onClick={() => setSelected(event)}><div className="event-thumb"><span>◉</span><i>{formatTime(event.start_time_ms)}</i></div><div className="event-copy"><div><strong>{event.event_type}</strong><span className={`severity ${event.severity}`}>{event.severity}</span></div><p>{event.analysis?.summary ?? '等待智能分析'}</p><small>{event.objects.length} 个目标 · 置信度 {(event.confidence * 100).toFixed(0)}%</small></div><span className="arrow">›</span></button>)}</div>}</div><div className="detail-column">{selected ? <><div className="detail-top"><div><p className="eyebrow">EVENT DETAIL / {selected.id}</p><h2>事件详情</h2></div><span className="status">{statusLabel(selected.status)}</span></div><div className="evidence"><div className="evidence-grid"><span className="box box-a" /><span className="box box-b" /><span className="scanline" /><div className="evidence-label">VIDEO EVIDENCE <b>{formatTime(selected.start_time_ms)} — {formatTime(selected.end_time_ms)}</b></div></div></div><div className="detail-info"><div><span>事件类型</span><strong>{selected.event_type}</strong></div><div><span>严重等级</span><strong className="danger">{selected.severity.toUpperCase()}</strong></div><div><span>检测目标</span><strong>{selected.objects.length} PERSON</strong></div><div><span>模型置信度</span><strong>{(selected.confidence * 100).toFixed(1)}%</strong></div></div><div className="analysis"><p className="eyebrow">MODEL ANALYSIS</p><h3>{selected.analysis?.summary}</h3><p>{selected.analysis?.suggestion}</p><small>来源：{selected.analysis?.report_source} · Prompt v1</small></div><div className="actions"><button className="confirm" onClick={() => reviewEvent('confirm')} disabled={reviewing || selected.status === 'confirmed'}>{reviewing ? '保存中…' : '确认事件'}</button><button onClick={() => reviewEvent('ignore')} disabled={reviewing || selected.status === 'ignored'}>{selected.status === 'ignored' ? '已忽略' : '忽略'}</button></div></> : <div className="empty detail-empty">选择一个事件查看证据与分析</div>}</div></section></main>
  </div>;
}
