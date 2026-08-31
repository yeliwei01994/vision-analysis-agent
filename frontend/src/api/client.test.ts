import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from './client';
import type { JobProgressEvent } from '../types/events';

class FakeEventSource {
  static instances: FakeEventSource[] = [];

  readonly url: string;
  closed = false;
  private listeners = new Map<string, Set<EventListenerOrEventListenerObject>>();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
    const listeners = this.listeners.get(type) ?? new Set<EventListenerOrEventListenerObject>();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject) {
    this.listeners.get(type)?.delete(listener);
  }

  close() {
    this.closed = true;
  }

  listenerCount(type: string) {
    return this.listeners.get(type)?.size ?? 0;
  }

  emit(type: string, data?: string) {
    const event =
      type === 'job-progress'
        ? new MessageEvent(type, { data })
        : new Event(type);

    for (const listener of this.listeners.get(type) ?? []) {
      if (typeof listener === 'function') {
        listener(event);
      } else {
        listener.handleEvent(event);
      }
    }
  }
}

afterEach(() => {
  FakeEventSource.instances = [];
  vi.unstubAllGlobals();
});

describe('api.subscribeJobProgress', () => {
  it('subscribes to the SSE stream, forwards events, updates connection state, and cleans up', () => {
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
    const onEvent = vi.fn<(event: JobProgressEvent) => void>();
    const onStateChange = vi.fn<(state: 'connected' | 'reconnecting') => void>();

    const unsubscribe = api.subscribeJobProgress(onEvent, onStateChange);
    const source = FakeEventSource.instances[0];

    expect(source.url).toBe('/api/v1/jobs/progress/stream');

    source.emit('open');
    source.emit(
      'job-progress',
      JSON.stringify({
        job_id: 'job-1',
        status: 'processing',
        stage: 'detecting',
        progress: 35,
        message: 'detecting objects',
        updated_at: 1725091200000,
        estimated_remaining_ms: null,
        sequence: 3,
      } satisfies JobProgressEvent),
    );
    source.emit('error');

    expect(onStateChange).toHaveBeenNthCalledWith(1, 'connected');
    expect(onEvent).toHaveBeenCalledWith({
      job_id: 'job-1',
      status: 'processing',
      stage: 'detecting',
      progress: 35,
      message: 'detecting objects',
      updated_at: 1725091200000,
      estimated_remaining_ms: null,
      sequence: 3,
    });
    expect(onStateChange).toHaveBeenNthCalledWith(2, 'reconnecting');

    unsubscribe();

    expect(source.listenerCount('open')).toBe(0);
    expect(source.listenerCount('job-progress')).toBe(0);
    expect(source.listenerCount('error')).toBe(0);
    expect(source.closed).toBe(true);
  });
});
