// Shared playback-session heartbeat, behind each client's player. It pings the
// server every 10 s (so the stream shows in the admin "En cours de lecture"
// panel), promptly on play/pause, signals stop on unmount, and listens for an
// admin "terminate" event (or a 410 on the next ping) to halt with a message.
//
// All platform-specific bits are injected: the web wrapper supplies browser-UA
// labels + an offset-aware position and drives the prompt ping off its React
// `playing` state; the TV player supplies platform labels + the raw <video> and
// drives the prompt ping off the element's play/pause events.

import { LumaApiError, type LumaClient, LumaEvents } from '@luma/core';
import { type RefObject, useEffect, useRef } from 'react';

export interface PlaybackHeartbeatParams {
  client: LumaClient;
  /** Gates pinging (web: signed-in; TV: `client.hasAuth`). */
  enabled: boolean;
  itemId: string;
  durationMs: number | null;
  /** Absolute current position in seconds (offset-aware on the web seamless stream). */
  getPosition: () => number;
  /** The current transport state (`buffering` = playing but stalled/rebuffering). */
  getState: () => 'playing' | 'paused' | 'buffering';
  /** Label of the audio track the viewer has selected (omit → keep server default). */
  getAudio?: () => string | undefined;
  /** Label of the selected subtitle track, or an "off" label (omit → unchanged). */
  getSubtitle?: () => string | undefined;
  mode: 'direct' | 'remux' | 'transcode';
  player: string;
  device: string;
  /** Base URL for the live-events stream (web: apiBase(); TV: client.baseUrl). */
  eventsBaseUrl: string;
  /** Session-id platform prefix, e.g. `'web'` | `'tv'`. */
  idPrefix: string;
  /** Fired the first time the session is terminated (admin stop, or a 410). */
  onTerminated: (message: string) => void;
  /** Element whose play/pause events trigger a prompt heartbeat (TV). */
  videoRef?: RefObject<HTMLVideoElement | null>;
  /** A value that, when it changes, triggers a prompt heartbeat (web passes its
   * React `playing` state). Omit to rely solely on `videoRef`. */
  pingSignal?: unknown;
}

export function usePlaybackHeartbeat(params: PlaybackHeartbeatParams): void {
  const ref = useRef(params);
  ref.current = params;

  const sessionId = useRef('');
  if (!sessionId.current) {
    sessionId.current = `${params.idPrefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  }
  // Once terminated we stop pinging and don't send a redundant stop on unmount.
  const terminated = useRef(false);
  const fireTerminated = useRef((message: string) => {
    if (terminated.current) return;
    terminated.current = true;
    ref.current.onTerminated(message);
  });

  const send = useRef(() => {
    const p = ref.current;
    if (!p.enabled || terminated.current) return;
    p.client
      .pingPlayback({
        sessionId: sessionId.current,
        itemId: p.itemId,
        positionMs: Math.round(p.getPosition() * 1000),
        durationMs: p.durationMs,
        state: p.getState(),
        mode: p.mode,
        player: p.player,
        device: p.device,
        audio: p.getAudio?.(),
        subtitle: p.getSubtitle?.(),
      })
      .catch((e: unknown) => {
        // 410 Gone → an admin terminated this session (WS fallback).
        if (e instanceof LumaApiError && e.status === 410) fireTerminated.current('');
      });
  });

  // Heartbeat loop + prompt ping on the element's play/pause + stop on unmount.
  // biome-ignore lint/correctness/useExhaustiveDependencies: send/sessionId are stable refs; re-run only on client/enabled.
  useEffect(() => {
    if (!params.enabled) return;
    const ping = () => send.current();
    ping();
    const iv = setInterval(ping, 10000);
    const sid = sessionId.current;
    const { client, videoRef } = params;
    const v = videoRef?.current;
    v?.addEventListener('play', ping);
    v?.addEventListener('pause', ping);
    return () => {
      clearInterval(iv);
      v?.removeEventListener('play', ping);
      v?.removeEventListener('pause', ping);
      if (!terminated.current) client.stopPlayback(sid).catch(() => undefined);
    };
  }, [params.client, params.enabled]);

  // Prompt ping when the caller's play state changes (web passes React `playing`).
  // biome-ignore lint/correctness/useExhaustiveDependencies: fire only on the signal change.
  useEffect(() => {
    if (ref.current.pingSignal === undefined) return;
    send.current();
  }, [params.pingSignal]);

  // Listen for an admin terminating this session (matched by session id).
  useEffect(() => {
    if (!params.enabled) return;
    const ev = new LumaEvents(params.eventsBaseUrl, {
      onEvent: (e) => {
        if (e.type === 'playback.terminate' && e.sessionId === sessionId.current) {
          fireTerminated.current(e.message);
        }
      },
    });
    ev.connect();
    return () => ev.close();
  }, [params.enabled, params.eventsBaseUrl]);
}
