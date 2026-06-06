// useWebSocket hook tests.
//
// We mock the global `WebSocket` constructor with a small in-test
// implementation so we can drive its lifecycle: open, message,
// close. The hook should:
//   1. Open a connection to `ws://<host>/ws` on mount.
//   2. Send a `subscribe` envelope listing the documented channels
//      when the socket opens.
//   3. Forward `event` messages to the feed store via addLiveEvent.
//   4. Skip non-event payloads (welcome, pong, error) without
//      throwing.

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useFeedStore } from '../store/feedStore';
import { useWebSocket } from './useWebSocket';

type Listener<T> = (event: T) => void;

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];

  url: string;
  readyState = 0; // CONNECTING
  sent: string[] = [];

  onopen: Listener<Event> | null = null;
  onmessage: Listener<MessageEvent> | null = null;
  onclose: Listener<CloseEvent> | null = null;
  onerror: Listener<Event> | null = null;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }

  send = vi.fn((data: string) => {
    this.sent.push(data);
  });

  close = vi.fn(() => {
    this.readyState = 3; // CLOSED
    this.onclose?.(new CloseEvent('close'));
  });

  // Test helpers
  open() {
    this.readyState = 1;
    this.onopen?.(new Event('open'));
  }

  message(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) } as MessageEvent);
  }
}

describe('useWebSocket', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
    useFeedStore.setState({ wsConnected: false });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('opens a WebSocket and subscribes to the documented channels', () => {
    renderHook(() => useWebSocket());

    expect(FakeWebSocket.instances).toHaveLength(1);
    const socket = FakeWebSocket.instances[0];
    expect(socket.url).toBe('ws://localhost:5173/ws');

    act(() => socket.open());

    expect(useFeedStore.getState().wsConnected).toBe(true);
    expect(socket.sent).toHaveLength(1);
    const envelope = JSON.parse(socket.sent[0]);
    expect(envelope).toEqual({
      type: 'subscribe',
      channels: ['events', 'broadcast', 'system'],
    });
  });

  it('forwards event messages to the feed store', () => {
    const addSpy = vi.spyOn(useFeedStore.getState(), 'addLiveEvent');
    renderHook(() => useWebSocket());
    const socket = FakeWebSocket.instances[0];
    act(() => socket.open());

    act(() =>
      socket.message({
        type: 'event',
        data: {
          subject: 'ingestion.document.received',
          event_type: 'ingestion.document.received',
          envelope: { document_id: 'doc-1' },
        },
      }),
    );

    expect(addSpy).toHaveBeenCalledWith({
      type: 'event',
      event_type: 'ingestion.document.received',
      subject: 'ingestion.document.received',
      envelope: { document_id: 'doc-1' },
    });
  });

  it('ignores non-event frames', () => {
    const addSpy = vi.spyOn(useFeedStore.getState(), 'addLiveEvent');
    renderHook(() => useWebSocket());
    const socket = FakeWebSocket.instances[0];
    act(() => socket.open());

    for (const frame of [
      { type: 'welcome', data: { server_started_at: 't', message: 'hi' } },
      { type: 'pong', data: { timestamp: 't' } },
      { type: 'error', data: { message: 'oops' } },
    ]) {
      act(() => socket.message(frame));
    }

    expect(addSpy).not.toHaveBeenCalled();
  });

  it('marks the connection as offline on close and reconnects', () => {
    vi.useFakeTimers();
    try {
      renderHook(() => useWebSocket());
      const first = FakeWebSocket.instances[0];
      act(() => first.open());
      expect(useFeedStore.getState().wsConnected).toBe(true);

      act(() => first.close());
      expect(useFeedStore.getState().wsConnected).toBe(false);

      // Reconnect attempt should fire on the exponential backoff
      // (1s for the first attempt).
      act(() => {
        vi.advanceTimersByTime(1100);
      });
      expect(FakeWebSocket.instances.length).toBeGreaterThan(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
