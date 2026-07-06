// Live server events over WebSocket (`/api/events`). The client holds this open
// and updates its UI in place no relaunch/refresh when the library changes
// (scan finished, metadata/art resolved). Auto-reconnects with backoff.

import type { StageStat } from './generated';

export type ServerEvent =
  | { type: 'hello'; version: string }
  | { type: 'scan.started' }
  | { type: 'scan.completed'; items: number; shows: number; libraries: number }
  | { type: 'library.updated' }
  | { type: 'item.updated'; id: string }
  | { type: 'show.updated'; id: string }
  | { type: 'enrich.progress'; done: number; total: number }
  | { type: 'enrich.completed'; resolved: number; total: number }
  | { type: 'probe.progress'; done: number; total: number }
  | { type: 'probe.completed'; total: number }
  | { type: 'playback.started'; count: number }
  | { type: 'playback.updated'; count: number }
  | { type: 'playback.stopped'; count: number }
  | { type: 'playback.terminate'; sessionId: string; message: string }
  | { type: 'settings.updated' }
  | { type: 'job.started'; key: string; runId: string }
  | { type: 'job.progress'; key: string; runId: string; done: number; total: number }
  | { type: 'job.log'; runId: string; level: string; message: string }
  | { type: 'job.finished'; key: string; runId: string; status: string }
  | { type: 'pipeline.stats'; stages: StageStat[] }
  | { type: 'request.updated'; id: string; status: string }
  | {
      type: 'download.progress';
      id: string;
      requestId: string | null;
      progress: number;
      downBps: number;
      upBps: number;
      peers: number;
      peersSeen: number;
      state: string;
    }
  | { type: 'download.completed'; id: string; title: string }
  | { type: 'vpn.status'; connected: boolean; exitIp: string | null; paused: boolean };

export interface LumaEventsOptions {
  onEvent?: (event: ServerEvent) => void;
  onOpen?: () => void;
  onClose?: () => void;
  /** Override the WebSocket implementation (e.g. in tests/SSR). */
  WebSocketImpl?: typeof WebSocket;
  /** Max reconnect backoff (ms). Default 15000. */
  maxBackoffMs?: number;
}

/** Reconnecting client for the LUMA server's event stream. */
export class LumaEvents {
  private readonly url: string;
  private readonly opts: LumaEventsOptions;
  private ws: WebSocket | null = null;
  private closed = false;
  private retry = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(baseUrl: string, opts: LumaEventsOptions = {}) {
    // http→ws, https→wss.
    this.url = `${baseUrl.replace(/^http/i, 'ws').replace(/\/+$/, '')}/api/events`;
    this.opts = opts;
  }

  connect(): void {
    if (this.closed) return;
    const WS = this.opts.WebSocketImpl ?? globalThis.WebSocket;
    if (!WS) return;

    let ws: WebSocket;
    try {
      ws = new WS(this.url);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.ws = ws;

    ws.onopen = () => {
      this.retry = 0;
      this.opts.onOpen?.();
    };
    ws.onmessage = (ev: MessageEvent) => {
      if (typeof ev.data !== 'string') return;
      try {
        this.opts.onEvent?.(JSON.parse(ev.data) as ServerEvent);
      } catch {
        /* ignore malformed frames */
      }
    };
    ws.onclose = () => {
      this.opts.onClose?.();
      this.scheduleReconnect();
    };
    ws.onerror = () => {
      try {
        ws.close();
      } catch {
        /* ignore */
      }
    };
  }

  private scheduleReconnect(): void {
    if (this.closed) return;
    const max = this.opts.maxBackoffMs ?? 15000;
    const delay = Math.min(1000 * 2 ** this.retry, max);
    this.retry += 1;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.connect(), delay);
  }

  close(): void {
    this.closed = true;
    clearTimeout(this.timer);
    try {
      this.ws?.close();
    } catch {
      /* ignore */
    }
  }
}
