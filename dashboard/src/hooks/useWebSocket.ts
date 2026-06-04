import { useEffect, useRef } from 'react';
import { useFeedStore } from '../store/feedStore';

// Wire-protocol types for the `/ws` endpoint. The Rust hub pushes
// these payloads as JSON text frames; see
// `crates/api-gateway/src/ws.rs` and `docs/api/internal-api.md` for
// the canonical contract.

type WsMessage =
  | { type: 'welcome'; data: { server_started_at: string; message: string } }
  | { type: 'pong'; data: { timestamp: string } }
  | { type: 'error'; data: { message: string } }
  | {
      type: 'event';
      data: {
        subject: string;
        event_type: string;
        envelope: Record<string, unknown>;
      };
    };

const CHANNELS: Array<'events' | 'broadcast' | 'system'> = [
  'events',
  'broadcast',
  'system',
];

export const useWebSocket = () => {
  const { setWsConnected, addLiveEvent } = useFeedStore();
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const attemptRef = useRef(0);

  useEffect(() => {
    const connect = () => {
      // Use relative protocol and host to align with Vite's proxy mapping
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const host = window.location.host;
      const wsUrl = `${protocol}//${host}/ws`;

      console.log(`Connecting to WebSocket at ${wsUrl}`);
      const socket = new WebSocket(wsUrl);
      socketRef.current = socket;

      socket.onopen = () => {
        console.log('WebSocket connection established');
        setWsConnected(true);
        attemptRef.current = 0;

        // Subscribe to the documented channel set. The server
        // returns a `welcome` envelope first; everything after is
        // a stream of bus events.
        socket.send(
          JSON.stringify({
            type: 'subscribe',
            channels: CHANNELS,
          }),
        );
      };

      socket.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as WsMessage;
          if (data.type === 'event') {
            addLiveEvent({
              type: 'event',
              event_type: data.data.event_type,
              subject: data.data.subject,
              envelope: data.data.envelope,
            });
          } else if (data.type === 'welcome') {
            console.log(
              `Realtime stream online (server_started_at=${data.data.server_started_at})`,
            );
          } else if (data.type === 'pong') {
            // Heartbeat reply; nothing to do.
          } else if (data.type === 'error') {
            console.warn('Realtime stream reported error', data.data.message);
          }
        } catch (err) {
          console.warn('Failed to parse WebSocket message data', err);
        }
      };

      socket.onclose = () => {
        console.log('WebSocket connection closed. Retrying...');
        setWsConnected(false);

        // Exponential backoff
        const timeout = Math.min(1000 * Math.pow(2, attemptRef.current), 15000);
        attemptRef.current += 1;

        reconnectTimeoutRef.current = window.setTimeout(() => {
          connect();
        }, timeout);
      };

      socket.onerror = (err) => {
        console.error('WebSocket encountered error: ', err);
        socket.close();
      };
    };

    connect();

    return () => {
      if (socketRef.current) {
        socketRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
    };
  }, [setWsConnected, addLiveEvent]);
};
