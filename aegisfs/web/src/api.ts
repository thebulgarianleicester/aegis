import { useCallback, useEffect, useRef, useState } from 'react';
import type { Telemetry } from './types';

export function useTelemetry() {
  const [telemetry, setTelemetry] = useState<Telemetry | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState('');
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const response = await fetch('/api/v1/telemetry');
      if (!response.ok) throw new Error(`API returned ${response.status}`);
      const value = await response.json() as Telemetry;
      if (value.status.schema_version !== 2) throw new Error('Unsupported telemetry schema');
      if (mounted.current) {
        setTelemetry(value);
        setError('');
      }
    } catch (reason) {
      if (mounted.current) {
        setConnected(false);
        setError(reason instanceof Error ? reason.message : 'Daemon unavailable');
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    let socket: WebSocket | null = null;
    let retry: number | undefined;
    let disposed = false;
    const connect = () => {
      if (disposed) return;
      const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      socket = new WebSocket(`${protocol}//${location.host}/api/v1/stream`);
      socket.onopen = () => { if (mounted.current) { setConnected(true); setError(''); } };
      socket.onmessage = event => {
        try {
          const value = JSON.parse(event.data) as Telemetry;
          if (value.status.schema_version === 2 && mounted.current) setTelemetry(value);
        } catch { /* Ignore malformed lab frames; HTTP polling remains available. */ }
      };
      socket.onerror = () => socket?.close();
      socket.onclose = () => {
        if (mounted.current) setConnected(false);
        if (!disposed) retry = window.setTimeout(connect, 2000);
      };
    };
    connect();
    const polling = window.setInterval(() => { if (!socket || socket.readyState !== WebSocket.OPEN) void refresh(); }, 4000);
    return () => {
      disposed = true;
      mounted.current = false;
      window.clearInterval(polling);
      if (retry) window.clearTimeout(retry);
      socket?.close();
    };
  }, [refresh]);

  return { telemetry, connected, error, refresh };
}

export async function post<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? '{}' : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error ?? `Request failed (${response.status})`);
  return value as T;
}
