import type { EventItem, EventRule, VideoJob } from '../types/events';

const request = async <T>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(path, { headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) }, ...init });
  if (!response.ok) throw new Error(`请求失败 (${response.status})`);
  return response.json() as Promise<T>;
};

export const api = {
  createVideo: (filename: string, duration_ms = 0) => request<VideoJob>('/api/v1/videos', { method: 'POST', body: JSON.stringify({ filename, duration_ms }) }),
  uploadVideo: async (file: File) => {
    const form = new FormData();
    form.append('file', file);
    const response = await fetch('/api/v1/videos/upload', { method: 'POST', body: form });
    if (!response.ok) throw new Error(`上传失败 (${response.status})`);
    return response.json() as Promise<VideoJob>;
  },
  processVideo: (id: string) => request<VideoJob>(`/api/v1/videos/${id}/process`, { method: 'POST' }),
  getJob: (id: string) => request<VideoJob>(`/api/v1/jobs/${id}`),
  listJobs: () => request<VideoJob[]>('/api/v1/jobs'),
  updateJob: (id: string, filename: string) => request<VideoJob>(`/api/v1/jobs/${id}`, { method: 'PUT', body: JSON.stringify({ filename }) }),
  deleteJob: async (id: string) => {
    const response = await fetch(`/api/v1/jobs/${id}`, { method: 'DELETE' });
    if (!response.ok) throw new Error(`请求失败 (${response.status})`);
  },
  deleteEvent: async (id: string) => {
    const response = await fetch(`/api/v1/events/${id}`, { method: 'DELETE' });
    if (!response.ok) throw new Error(`请求失败 (${response.status})`);
  },
  listEvents: () => request<EventItem[]>('/api/v1/events'),
  searchEvents: (keyword: string) => request<EventItem[]>('/api/v1/events/search', { method: 'POST', body: JSON.stringify({ keyword }) }),
  listRules: () => request<EventRule[]>('/api/v1/event-rules'),
  updateRule: (rule: EventRule) => request<EventRule>(`/api/v1/event-rules/${rule.event_type}`, { method: 'PUT', body: JSON.stringify({ class_name: rule.class_name, min_confidence: rule.min_confidence, min_duration_ms: rule.min_duration_ms }) }),
  confirmEvent: (id: string) => request<EventItem>(`/api/v1/events/${id}/confirm`, { method: 'POST' }),
  ignoreEvent: (id: string) => request<EventItem>(`/api/v1/events/${id}/ignore`, { method: 'POST' }),
};
