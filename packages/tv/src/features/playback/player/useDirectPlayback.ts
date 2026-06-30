import {
  type AudioTrack,
  attachDirectPlay,
  audioTracksOf,
  canDirectPlay,
  type DirectPlayVerdict,
  type LumaClient,
  type MediaItem,
  type MessageKey,
  planAudio,
  restorePlaybackAfterSwap,
} from '@luma/core';
import { usePlaybackHeartbeat } from '@luma/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useResumeAndPersist } from '#tv/features/playback/player/useResumeAndPersist';

export interface Playback {
  videoRef: React.RefObject<HTMLVideoElement>;
  verdict: DirectPlayVerdict | null;
  /** Codec/stream load failure, as an i18n key translated at the render site. */
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
  /** Seek by `delta` seconds immediately, clamped to [0, duration]. */
  seek: (delta: number) => void;
  /** Progressive seek: accumulate an offset in direction `dir` (the step grows on
   * rapid successive calls hold to go faster) and commit one real seek once the
   * user stops. `seekPreview` is the pending absolute position while seeking. */
  nudge: (dir: -1 | 1) => void;
  /** Pending absolute position (s) during a progressive seek, else `null`. */
  seekPreview: number | null;
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
  // the subtitle overlay on-device so the TV stays on the per-track remux path
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
    // playback actually starts, then stop so we never fight a real pause (these
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
  // track list no source reload, so the video keeps playing at the same spot.
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

  // Restore the saved resume position, then persist progress (interval + on
  // pause / ~finish / unmount). Shares the engine's <video> ref.
  useResumeAndPersist(client, item, videoRef);

  // Heartbeat the session for the admin dashboard's "En cours de lecture" panel
  // and react to a remote admin termination (pause + surface the message). The
  // loop/terminate plumbing is shared with the web player (@luma/ui); the TV
  // supplies platform labels, the raw <video> position, and play/pause events.
  const tvDevice = (): string => {
    const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent || '';
    if (/Tizen/i.test(ua)) return 'Samsung TV';
    if (/web0?s|LG/i.test(ua)) return 'LG TV';
    return 'TV';
  };
  usePlaybackHeartbeat({
    client,
    enabled: client.hasAuth,
    itemId: item.id,
    durationMs: item.durationMs ?? null,
    getPosition: () => videoRef.current?.currentTime ?? 0,
    getState: () => (videoRef.current?.paused ? 'paused' : 'playing'),
    mode: 'direct',
    player: 'LUMA TV',
    device: tvDevice(),
    eventsBaseUrl: client.baseUrl,
    idPrefix: 'tv',
    videoRef,
    onTerminated: (message) => {
      try {
        videoRef.current?.pause();
      } catch {
        /* ignore */
      }
      // Empty string → the render site supplies the localized default.
      setTerminated(message.trim() || '');
    },
  });

  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  }, []);

  // Catalogue runtime (s), preferred over v.duration (Infinity for a growing HLS
  // remux → would never clamp / desyncs the bar).
  const runtime = useCallback(() => {
    const v = videoRef.current;
    if (item.durationMs) return item.durationMs / 1000;
    if (v && Number.isFinite(v.duration)) return v.duration;
    return 0;
  }, [item]);

  const seek = useCallback(
    (delta: number) => {
      const v = videoRef.current;
      if (!v) return;
      const total = runtime();
      const target = v.currentTime + delta;
      v.currentTime = Math.max(0, total ? Math.min(total, target) : target);
    },
    [runtime],
  );

  // ----- progressive (accelerating) seek --------------------------------------
  // Repeated/held direction keys would otherwise fire a real seek each press
  // each one re-buffers on direct play, so fast-seeking stutters. Instead we
  // accumulate a preview offset (the step grows the longer you hold) and commit a
  // single seek ~450 ms after the last press.
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const seekRef = useRef<{ target: number; step: number; last: number } | null>(null);
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const commitSeek = useCallback(() => {
    if (commitTimer.current) {
      clearTimeout(commitTimer.current);
      commitTimer.current = null;
    }
    const s = seekRef.current;
    seekRef.current = null;
    setSeekPreview(null);
    const v = videoRef.current;
    if (s && v) v.currentTime = s.target;
  }, []);

  const nudge = useCallback(
    (dir: -1 | 1) => {
      const v = videoRef.current;
      if (!v) return;
      const total = runtime();
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
      let s = seekRef.current;
      if (!s || now - s.last > 700) {
        // Fresh session start at the live position, base step.
        s = { target: v.currentTime, step: 5, last: now };
      } else {
        // Held / rapid presses → accelerate gently (5 → 7 → 9.8 → … → 120 s).
        s.step = Math.min(s.step * 1.4, 120);
      }
      const raw = s.target + dir * s.step;
      s.target = Math.max(0, total > 0 ? Math.min(total - 1, raw) : raw);
      s.last = now;
      seekRef.current = s;
      setSeekPreview(s.target);
      if (commitTimer.current) clearTimeout(commitTimer.current);
      commitTimer.current = setTimeout(commitSeek, 450);
    },
    [runtime, commitSeek],
  );

  // Flush a pending seek if the player unmounts mid-gesture.
  useEffect(() => () => commitSeek(), [commitSeek]);

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
    nudge,
    seekPreview,
  };
}
