import {
  type AudioTrack,
  attachDirectPlay,
  audioTracksOf,
  canDirectPlay,
  type DirectPlayVerdict,
  type LumaClient,
  LumaEvents,
  type MediaItem,
  type MessageKey,
  planAudio,
  restorePlaybackAfterSwap,
} from '@luma/core';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface Playback {
  videoRef: React.RefObject<HTMLVideoElement>;
  verdict: DirectPlayVerdict | null;
  /** Codec/stream load failure, as an i18n key — translated at the render site. */
  error: MessageKey | null;
  /** Admin-terminated message: a custom string, or '' for the default (the render
   * site supplies the localized fallback). Null while the session is live. */
  terminated: string | null;
  playing: boolean;
  waiting: boolean;
  cur: number;
  dur: number;
  bufEnd: number;
  /** Every audio track, for the picker. */
  audioTracks: AudioTrack[];
  /** Index of the currently-selected audio track. */
  audioIndex: number;
  /** Switch to the audio track with this audio-relative index. */
  setAudio: (index: number) => void;
  togglePlay: () => void;
  /** Seek by `delta` seconds, clamped to [0, duration]. */
  seek: (delta: number) => void;
}

/**
 * Direct-play a media item in a `<video>`: attaches the source, mirrors the
 * element's playback state into React, restores the saved resume position, and
 * persists progress (every 10 s, on pause/ended, and on unmount).
 */
export function useDirectPlayback(client: LumaClient, item: MediaItem): Playback {
  const videoRef = useRef<HTMLVideoElement>(null);

  const [verdict, setVerdict] = useState<DirectPlayVerdict | null>(null);
  const [error, setError] = useState<MessageKey | null>(null);
  const [terminated, setTerminated] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(true);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(item.durationMs ? item.durationMs / 1000 : 0);
  const [bufEnd, setBufEnd] = useState(0);
  const [audioIndex, setAudioIndex] = useState(() => {
    const tracks = audioTracksOf(item);
    return (tracks.find((t) => t.default) ?? tracks[0])?.index ?? 0;
  });

  const audioTracks = audioTracksOf(item);
  // NOTE: Tizen/webOS HTML5 <video> can't switch HLS alternate-audio renditions
  // in place (that needs Samsung's AVPlay API), and the master stream also broke
  // the subtitle overlay on-device — so the TV stays on the per-track remux path
  // (reload on switch). The seamless HLS-master path is web-only (see the web
  // useVideoPlayback). Keeping this flag false until an AVPlay track-switch lands.
  const seamless = false;

  // Mirror element state into React. Source attachment lives in its own effect
  // (below) so switching the audio track re-attaches without tearing down the
  // listeners or resetting autoplay resilience.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;

    // Resilient autoplay: a single play() can reject when it races ahead of the
    // stream being ready (or a transient autoplay block), leaving the video paused
    // until the user presses Play. Keep retrying on the media-ready events until
    // playback actually starts, then stop — so we never fight a real pause (these
    // events only fire during initial load / seeks, not on a user-initiated pause).
    let started = false;
    const tryAutoplay = () => {
      if (started || !v.paused) return;
      const p = v.play();
      if (p && typeof p.then === 'function') p.catch(() => undefined);
    };

    const onTime = () => setCur(v.currentTime);
    // Prefer the catalogue runtime: a per-track HLS remux is a growing event
    // playlist whose `video.duration` is Infinity/growing, which desyncs the bar.
    const total = item.durationMs ? item.durationMs / 1000 : 0;
    const onDur = () => {
      if (total > 0) setDur(total);
      else if (Number.isFinite(v.duration)) setDur(v.duration);
    };
    const onProg = () => setBufEnd(v.buffered.length ? v.buffered.end(v.buffered.length - 1) : 0);
    const onPlay = () => {
      started = true;
      setPlaying(true);
    };
    const onPause = () => setPlaying(false);
    const onWaiting = () => setWaiting(true);
    const onPlaying = () => setWaiting(false);
    const onErr = () => setError('player.cantPlay');

    v.addEventListener('timeupdate', onTime);
    v.addEventListener('durationchange', onDur);
    v.addEventListener('progress', onProg);
    v.addEventListener('play', onPlay);
    v.addEventListener('pause', onPause);
    v.addEventListener('waiting', onWaiting);
    v.addEventListener('playing', onPlaying);
    v.addEventListener('error', onErr);
    v.addEventListener('loadedmetadata', tryAutoplay);
    v.addEventListener('loadeddata', tryAutoplay);
    v.addEventListener('canplay', tryAutoplay);
    return () => {
      v.removeEventListener('timeupdate', onTime);
      v.removeEventListener('durationchange', onDur);
      v.removeEventListener('progress', onProg);
      v.removeEventListener('play', onPlay);
      v.removeEventListener('pause', onPause);
      v.removeEventListener('waiting', onWaiting);
      v.removeEventListener('playing', onPlaying);
      v.removeEventListener('error', onErr);
      v.removeEventListener('loadedmetadata', tryAutoplay);
      v.removeEventListener('loadeddata', tryAutoplay);
      v.removeEventListener('canplay', tryAutoplay);
    };
  }, [client, item]);

  // Source wiring. Seamless mode points the native player at the HLS master ONCE
  // (every audio track is a rendition); switching language happens IN PLACE (see
  // the next effect) so the source is never re-pointed and the picture never
  // moves. Otherwise it's direct-play, or a per-track remux that reloads on
  // switch (the fallback for audio this runtime can't decode). The deps gate
  // `audioIndex` out in seamless mode so a switch never re-attaches.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    setVerdict(canDirectPlay(item));

    if (seamless) {
      // Tizen/webOS decode AC3/EAC3 natively, so play the master via <video src>
      // (NOT MSE/hls.js, which can't decode those). Renditions surface on
      // video.audioTracks for in-place selection.
      v.src = client.hlsMasterUrl(item.id);
      v.preload = 'auto';
      const p = v.play();
      if (p && typeof p.then === 'function') p.catch(() => undefined);
      return;
    }

    const plan = planAudio(item, audioIndex);
    // Capture position + play state BEFORE re-pointing the source (assigning the
    // new src resets currentTime to 0).
    const resumeAt = v.currentTime || 0;
    const wasPlaying = !v.paused;

    if (plan.mode === 'direct') {
      attachDirectPlay(v, client, item, { autoplay: true });
    } else {
      v.src = client.hlsAudioUrl(item.id, plan.index, plan.copy);
      v.preload = 'auto';
      const p = v.play();
      if (p && typeof p.then === 'function') p.catch(() => undefined);
    }
    // Restore the old position once the new source makes it seekable.
    return restorePlaybackAfterSwap(v, resumeAt, wasPlaying);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, item, seamless, seamless ? 0 : audioIndex]);

  // Seamless language switch: select the rendition IN PLACE via the native audio
  // track list — no source reload, so the video keeps playing at the same spot.
  useEffect(() => {
    if (!seamless) return;
    const order = audioTracksOf(item);
    const rendition = Math.max(
      0,
      order.findIndex((t) => t.index === audioIndex),
    );
    type NativeAudioTracks = { length: number; [i: number]: { enabled: boolean } };
    const tracks = (videoRef.current as unknown as { audioTracks?: NativeAudioTracks } | null)
      ?.audioTracks;
    if (tracks?.length) {
      for (let i = 0; i < tracks.length; i += 1) tracks[i]!.enabled = i === rendition;
    }
  }, [item, seamless, audioIndex]);

  // Restore the saved resume position once metadata is available.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !client.hasAuth) return;
    let cancelled = false;
    let applied = false;
    const apply = (sec: number) => {
      if (applied) return;
      applied = true;
      if (v.currentTime < sec - 2) v.currentTime = sec;
    };
    client
      .itemProgress(item.id)
      .then((p) => {
        if (cancelled || !p) return;
        const durMs = p.durationMs ?? item.durationMs ?? 0;
        const posSec = p.positionMs / 1000;
        if (posSec > 15 && (!durMs || p.positionMs < durMs * 0.95)) {
          if (v.readyState >= 1) apply(posSec);
          else v.addEventListener('loadedmetadata', () => apply(posSec), { once: true });
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, item]);

  const saveProgress = useCallback(() => {
    const v = videoRef.current;
    if (!v || !client.hasAuth) return;
    const d = v.duration;
    const pos = v.currentTime;
    if (!Number.isFinite(d) || d <= 0 || pos < 5) return;
    if (pos > d * 0.97) void client.deleteProgress(item.id);
    else void client.saveProgress(item.id, pos * 1000, d * 1000);
  }, [client, item]);

  // Persist every 10 s, on pause, on ~finish, and on exit (cleanup).
  useEffect(() => {
    if (!client.hasAuth) return;
    const v = videoRef.current;
    const interval = setInterval(saveProgress, 10000);
    const onEnded = () => void client.deleteProgress(item.id);
    v?.addEventListener('pause', saveProgress);
    v?.addEventListener('ended', onEnded);
    return () => {
      clearInterval(interval);
      v?.removeEventListener('pause', saveProgress);
      v?.removeEventListener('ended', onEnded);
      saveProgress();
    };
  }, [client, item, saveProgress]);

  // Heartbeat the session to the server so it shows in the admin dashboard's
  // "En cours de lecture" panel (every 10 s + on play/pause; stop on unmount).
  const sessionId = useRef<string>('');
  if (!sessionId.current) {
    sessionId.current = `tv-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  }
  // Set once an admin terminates this session: stops pinging + shows the message.
  const terminatedRef = useRef(false);
  useEffect(() => {
    if (!client.hasAuth) return;
    const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent || '';
    let device = 'TV';
    if (/Tizen/i.test(ua)) device = 'Samsung TV';
    else if (/web0?s|LG/i.test(ua)) device = 'LG TV';
    const ping = () => {
      const v = videoRef.current;
      if (!v || terminatedRef.current) return;
      client
        .pingPlayback({
          sessionId: sessionId.current,
          itemId: item.id,
          positionMs: Math.round((v.currentTime || 0) * 1000),
          durationMs: item.durationMs ?? null,
          state: v.paused ? 'paused' : 'playing',
          mode: 'direct',
          player: 'LUMA TV',
          device,
        })
        .catch(() => undefined);
    };
    ping();
    const iv = setInterval(ping, 10000);
    const v = videoRef.current;
    v?.addEventListener('play', ping);
    v?.addEventListener('pause', ping);
    const sid = sessionId.current;
    return () => {
      clearInterval(iv);
      v?.removeEventListener('play', ping);
      v?.removeEventListener('pause', ping);
      if (!terminatedRef.current) client.stopPlayback(sid).catch(() => undefined);
    };
  }, [client, item]);

  // An admin can remotely terminate this session → pause + surface the message.
  useEffect(() => {
    if (!client.hasAuth) return;
    const events = new LumaEvents(client.baseUrl, {
      onEvent: (e) => {
        if (e.type === 'playback.terminate' && e.sessionId === sessionId.current) {
          terminatedRef.current = true;
          try {
            videoRef.current?.pause();
          } catch {
            /* ignore */
          }
          // Empty string → the render site supplies the localized default.
          setTerminated(e.message?.trim() || '');
        }
      },
    });
    events.connect();
    return () => events.close();
  }, [client]);

  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  }, []);

  const seek = useCallback(
    (delta: number) => {
      const v = videoRef.current;
      if (!v) return;
      const target = v.currentTime + delta;
      // Clamp to the catalogue runtime, not v.duration (Infinity for the growing
      // HLS remux → would never clamp / desyncs the bar).
      let total: number;
      if (item.durationMs) total = item.durationMs / 1000;
      else if (Number.isFinite(v.duration)) total = v.duration;
      else total = 0;
      const max = total || target;
      v.currentTime = Math.max(0, Math.min(max, target));
    },
    [item],
  );

  const setAudio = useCallback(
    (index: number) => setAudioIndex((cur) => (cur === index ? cur : index)),
    [],
  );

  return {
    videoRef,
    verdict,
    error,
    terminated,
    playing,
    waiting,
    cur,
    dur,
    bufEnd,
    audioTracks,
    audioIndex,
    setAudio,
    togglePlay,
    seek,
  };
}
