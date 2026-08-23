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
  listEvents: () => request<EventItem[]>('/api/v1/events'),
  searchEvents: (keyword: string) => request<EventItem[]>('/api/v1/events/search', { method: 'POST', body: JSON.stringify({ keyword }) }),
  listRules: () => request<EventRule[]>('/api/v1/event-rules'),
  confirmEvent: (id: string) => request<EventItem>(`/api/v1/events/${id}/confirm`, { method: 'POST' }),
  ignoreEvent: (id: string) => request<EventItem>(`/api/v1/events/${id}/ignore`, { method: 'POST' }),
};
