import { useState } from 'react';
import { api } from '../api/client';
import type { EventItem, EventRule, VideoJob } from '../types/events';

function formatTime(ms: number) { return `${Math.floor(ms / 1000 / 60).toString().padStart(2, '0')}:${Math.floor(ms / 1000 % 60).toString().padStart(2, '0')}`; }

export function JobsPage({ jobs, onRefresh }: { jobs: VideoJob[]; onRefresh: () => Promise<void> }) {
  const [refreshing, setRefreshing] = useState(false);
  const [editing, setEditing] = useState<VideoJob | null>(null);
  const [filename, setFilename] = useState('');
  const [deleting, setDeleting] = useState<VideoJob | null>(null);
  const [mutating, setMutating] = useState(false);
  const [error, setError] = useState('');
  async function refresh() { setRefreshing(true); try { await onRefresh(); } finally { setRefreshing(false); } }
  async function saveEdit() {
    if (!editing) return;
    const nextFilename = filename.trim();
    if (!nextFilename || nextFilename.length > 255) { setError('文件名不能为空且不能超过 255 个字符'); return; }
    setMutating(true); setError('');
    try { await api.updateJob(editing.id, nextFilename); setEditing(null); await onRefresh(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '任务更新失败'); }
    finally { setMutating(false); }
  }
  async function confirmDelete() {
    if (!deleting) return;
    setMutating(true); setError('');
    try { await api.deleteJob(deleting.id); setDeleting(null); await onRefresh(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : '任务删除失败'); }
    finally { setMutating(false); }
  }
  return <section className="page-panel"><div className="page-heading"><div><p className="eyebrow">VIDEO TASKS</p><h2>视频任务</h2><p>查看上传记录、处理状态和分析进度。</p></div><button className="primary" onClick={refresh} disabled={refreshing}>{refreshing ? '刷新中…' : '刷新任务'}</button></div>{error && <div className="notice" role="alert">{error}</div>}<div className="data-table"><div className="data-row data-header"><span>文件名</span><span>状态</span><span>进度</span><span>时长</span><span>操作</span></div>{jobs.length === 0 ? <div className="empty">暂无视频任务</div> : jobs.map((job) => <div className="data-row" key={job.id}><strong>{job.filename}</strong><span className={`job-status ${job.status}`}>{job.status}</span><span>{job.progress}%</span><span>{formatTime(job.duration_ms)}</span><span className="row-actions"><button aria-label={`编辑 ${job.filename}`} onClick={() => { setEditing(job); setFilename(job.filename); setError(''); }}>编辑</button><button aria-label={`删除 ${job.filename}`} onClick={() => { setDeleting(job); setError(''); }} disabled={job.status === 'processing'}>删除</button></span></div>)}</div>{editing && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true" aria-labelledby="edit-job-title"><h3 id="edit-job-title">编辑视频任务</h3><label>任务文件名<input aria-label="任务文件名" value={filename} onChange={(event) => setFilename(event.target.value)} autoFocus /></label><div className="modal-actions"><button onClick={() => setEditing(null)}>取消</button><button className="confirm" onClick={saveEdit} disabled={mutating}>{mutating ? '保存中…' : '保存修改'}</button></div></div></div>}{deleting && <div className="modal-backdrop"><div className="modal" role="dialog" aria-modal="true" aria-labelledby="delete-job-title"><h3 id="delete-job-title">确认删除任务？</h3><p>任务“{deleting.filename}”将从列表和事件中移除，物理视频文件会保留。</p><div className="modal-actions"><button onClick={() => setDeleting(null)}>取消</button><button className="danger-button" onClick={confirmDelete} disabled={mutating}>{mutating ? '删除中…' : '确认删除'}</button></div></div></div>}</section>;
}

export function RulesPage({ rules, events, onSaved }: { rules: EventRule[]; events: EventItem[]; onSaved: () => Promise<void> }) {
  const [drafts, setDrafts] = useState<Record<string, EventRule>>(() => Object.fromEntries(rules.map((rule) => [rule.event_type, { ...rule }])));
  const [saving, setSaving] = useState('');
  const [editing, setEditing] = useState<EventRule | null>(null);
  const [background, setBackground] = useState<'blank' | 'evidence'>('blank');
  const [tool, setTool] = useState<'polygon' | 'rectangle'>('polygon');
  const [points, setPoints] = useState<[number, number][]>([]);
  function update(eventType: string, field: 'min_confidence' | 'min_duration_ms' | 'threshold', value: number) { setDrafts((current) => ({ ...current, [eventType]: { ...current[eventType], [field]: value } })); }
  async function save(rule: EventRule) { setSaving(rule.event_type); try { await api.updateRule(rule); await onSaved(); } finally { setSaving(''); } }
  function openEditor(rule: EventRule) { setEditing(rule); setPoints(rule.geometry?.points ?? []); setBackground('blank'); }
  function addPoint(event: React.MouseEvent<HTMLDivElement>) { const rect = event.currentTarget.getBoundingClientRect(); const point: [number, number] = [(event.clientX - rect.left) / rect.width, (event.clientY - rect.top) / rect.height]; if (tool === 'rectangle') { if (points.length === 1) { const [x, y] = points[0]; const [right, bottom] = point; setPoints([[x, y], [right, y], [right, bottom], [x, bottom]]); } else setPoints([point]); } else setPoints(current => [...current, point]); }
  const evidenceUrl = events.flatMap(event => event.evidence.frames ?? []).find(frame => frame.image_url)?.image_url;
  return <section className="page-panel"><div className="page-heading"><div><p className="eyebrow">EVENT RULES</p><h2>规则配置</h2><p>配置区域、阈值和启用状态。</p></div></div><div className="rule-grid">{rules.length === 0 ? <div className="empty">暂无规则</div> : rules.map((rule) => { const draft = drafts[rule.event_type] ?? rule; return <div className="rule-card" key={rule.event_type}><div><strong>{rule.event_type}</strong><small>目标：{rule.class_name} · {rule.version}</small></div><label>最低置信度<input type="number" min="0" max="1" step="0.01" value={draft.min_confidence} onChange={(event) => update(rule.event_type, 'min_confidence', Number(event.target.value))} /></label><label>最短持续时间（毫秒）<input type="number" min="0" step="100" value={draft.min_duration_ms} onChange={(event) => update(rule.event_type, 'min_duration_ms', Number(event.target.value))} /></label>{rule.event_type === 'person_count_limit' && <label>人数上限<input type="number" min="1" value={draft.threshold ?? 1} onChange={(event) => update(rule.event_type, 'threshold', Number(event.target.value))} /></label>}<label><input type="checkbox" checked={draft.enabled ?? true} onChange={(event) => setDrafts(current => ({ ...current, [rule.event_type]: { ...draft, enabled: event.target.checked } }))} />启用规则</label><button onClick={() => openEditor(draft)}>编辑区域</button><button className="confirm" onClick={() => save(draft)} disabled={saving === rule.event_type}>{saving === rule.event_type ? '保存中…' : '保存规则'}</button></div>; })}</div>{editing && <div className="modal-backdrop"><div className="modal zone-modal" role="dialog"><h3>编辑区域：{editing.event_type}</h3><div className="row-actions"><button onClick={() => setBackground('blank')}>空白画布</button><button disabled={!evidenceUrl} onClick={() => setBackground('evidence')}>视频证据背景</button><button onClick={() => setTool('polygon')}>多边形</button><button onClick={() => setTool('rectangle')}>矩形</button><button onClick={() => setPoints([])}>重置</button></div><div className="zone-canvas" onClick={addPoint} style={background === 'evidence' && evidenceUrl ? { backgroundImage: `url(${evidenceUrl})` } : undefined}><svg viewBox="0 0 100 100" preserveAspectRatio="none"><polygon points={points.map(([x, y]) => `${x * 100},${y * 100}`).join(' ')} /></svg></div><p>{points.length < 3 ? '请至少绘制 3 个点' : `已绘制 ${points.length} 个点`}</p><div className="modal-actions"><button onClick={() => setEditing(null)}>取消</button><button className="confirm" disabled={points.length < 3} onClick={() => { const next = { ...editing, geometry: { kind: 'polygon' as const, points } }; setDrafts(current => ({ ...current, [editing.event_type]: next })); setEditing(null); }}>应用区域</button></div></div></div>}</section>;
}

export function ModelsPage() {
  return <section className="page-panel"><div className="page-heading"><div><p className="eyebrow">MODEL REGISTRY</p><h2>模型版本</h2><p>查看当前检测器、分析器和规则版本。</p></div></div><div className="model-grid"><div className="model-card"><span>检测器</span><strong>mock-detector-v1</strong><small className="planned">待接入 YOLO</small></div><div className="model-card"><span>分析器</span><strong>mock-analyzer-v1</strong><small className="planned">待接入大模型</small></div><div className="model-card"><span>规则引擎</span><strong>rule-v1</strong><small className="online">当前使用</small></div></div></section>;
}
