import { LumaApiError, LumaEvents } from '@luma/core';
import { useEffect, useRef } from 'react';
import { apiBase } from '#web/lib/api';
import type { MovieView } from '#web/lib/api';
import { useAuth } from '#web/lib/auth';

// Heartbeat the current playback to the server so it shows up in the admin
// dashboard's "En cours de lecture" panel. Pings every 10 s while the player is
// open, immediately on play/pause transitions, and signals stop on unmount.
// Also listens for an admin "terminate" event (or a 410 on the next ping) so a
// remotely-stopped stream halts with a message.

interface Params {
  item: MovieView;
  /** Absolute current position in seconds (offset-aware). */
  getPosition: () => number;
  playing: boolean;
  /** `direct` (range-stream) or `transcode` (HLS audio re-encode). */
  mode: 'direct' | 'transcode';
  /** Called when an admin terminates this session. `message` may be empty. */
  onTerminated?: (message: string) => void;
}

function uaInfo(): { player: string; device: string } {
  if (typeof navigator === 'undefined') return { player: 'LUMA Web', device: 'Navigateur' };
  const ua = navigator.userAgent;
  let player = 'Navigateur';
  if (/Edg\//.test(ua)) player = 'Edge';
  else if (/Firefox\//.test(ua)) player = 'Firefox';
  else if (/Chrome\//.test(ua)) player = 'Chrome';
  else if (/Safari\//.test(ua)) player = 'Safari';
  let device = 'Web';
  if (/iPhone|iPad/.test(ua)) device = 'iOS';
  else if (/Android/.test(ua)) device = 'Android';
  else if (/Mac OS X/.test(ua)) device = 'macOS';
  else if (/Windows/.test(ua)) device = 'Windows';
  else if (/Linux/.test(ua)) device = 'Linux';
  return { player, device };
}

let counter = 0;

export function usePlaybackSession(params: Params): void {
  const { client, user } = useAuth();
  const sessionId = useRef<string>('');
  if (!sessionId.current) {
    sessionId.current = `web-${Date.now().toString(36)}-${(counter++).toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  }
  const ref = useRef(params);
  ref.current = params;
  // Once terminated we stop pinging and don't send a redundant stop on unmount.
  const terminated = useRef(false);

  const fireTerminated = useRef((message: string) => {
    if (terminated.current) return;
    terminated.current = true;
    ref.current.onTerminated?.(message);
  });

  const send = useRef(() => {
    if (!user || terminated.current) return;
    const p = ref.current;
    const ua = uaInfo();
    client
      .pingPlayback({
        sessionId: sessionId.current,
        itemId: p.item.id,
        positionMs: Math.round(p.getPosition() * 1000),
        durationMs: p.item.durationMs ?? null,
        state: p.playing ? 'playing' : 'paused',
        mode: p.mode,
        player: ua.player,
        device: ua.device,
      })
      .catch((e: unknown) => {
        // 410 Gone → an admin terminated this session (WS fallback).
        if (e instanceof LumaApiError && e.status === 410) fireTerminated.current('');
      });
  });

  // Heartbeat loop + stop on unmount.
  useEffect(() => {
    if (!user) return;
    const ping = () => send.current();
    ping();
    const iv = setInterval(ping, 10000);
    const sid = sessionId.current;
    return () => {
      clearInterval(iv);
      if (!terminated.current) client.stopPlayback(sid).catch(() => undefined);
    };
  }, [client, user]);

  // Promptly reflect play/pause transitions.
  useEffect(() => {
    send.current();
  }, [params.playing]);

  // Listen for an admin terminating this session (matched by session id).
  useEffect(() => {
    if (!user) return;
    const ev = new LumaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'playback.terminate' && e.sessionId === sessionId.current) {
          fireTerminated.current(e.message);
        }
      },
    });
    ev.connect();
    return () => ev.close();
  }, [user]);
}
