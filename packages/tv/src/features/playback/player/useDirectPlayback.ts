import {
  type AudioTrack,
  audioTrackId,
  audioTracksOf,
  canDirectPlay,
  type DirectPlayVerdict,
  type LumaClient,
  type MediaItem,
  type MessageKey,
  MSE_CAPS,
  masterNeedsAac,
  NATIVE_TV_CAPS,
  type PlayEnv,
  resolveAudioRelativeIndex,
  selectEngine,
} from '@luma/core';
import { usePlaybackHeartbeat } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AvplayEngine } from '#tv/features/playback/player/avplayEngine';
import {
  avplayAvailable,
  type EngineListeners,
  type TvEngine,
} from '#tv/features/playback/player/engine';
import { HtmlEngine } from '#tv/features/playback/player/htmlEngine';
import { useResumeAndPersist } from '#tv/features/playback/player/useResumeAndPersist';

export interface Playback {
  /** The HTML `<video>` surface (HTML engine). Null while the AVPlay surface is used. */
  videoRef: React.RefObject<HTMLVideoElement>;
  /** The AVPlay `<object>` surface (native Tizen engine). */
  objectRef: React.RefObject<HTMLObjectElement>;
  /** Which surface to render. */
  surface: 'video' | 'avplay';
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
  /** Index of the currently-selected audio track (audio-relative). */
  audioIndex: number;
  /** Switch to the audio track with this audio-relative index. */
  setAudio: (index: number) => void;
  togglePlay: () => void;
  /** Seek by `delta` seconds immediately, clamped to [0, duration]. */
  seek: (delta: number) => void;
  /** Seek to an ABSOLUTE position in seconds, clamped. */
  seekTo: (absSec: number) => void;
  /** Read the absolute current position in seconds. */
  getPosition: () => number;
  /** Progressive seek: accumulate an offset in direction `dir` (the step grows on
   * rapid successive calls hold to go faster) and commit one real seek the moment
   * the key is released. `seekPreview` is the pending absolute position. */
  nudge: (dir: -1 | 1) => void;
  /** Pending absolute position (s) during a progressive seek, else `null`. */
  seekPreview: number | null;
  /** Increments each time playback reaches the end (drives up-next autoplay). */
  endedNonce: number;
  /** Increments on every committed seek (re-anchors the subtitle cue pointer). */
  seekNonce: number;
}

/** The browser/platform environment for engine selection (TVs are Chromium). */
function detectTvEnv(): PlayEnv {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  const webos =
    /web0?s/i.test(ua) || typeof (globalThis as Record<string, unknown>).webOS !== 'undefined';
  return { platform: webos ? 'webos' : 'tizen', safari: false };
}

/** A plain, single-audio MP4 a bare TV `<video>` direct-plays natively. */
function tvDirectPlay(item: MediaItem): boolean {
  const container = (item.container ?? '').toLowerCase();
  if (container !== 'mp4' && container !== 'mov' && container !== 'm4v') return false;
  if (!canDirectPlay(item, NATIVE_TV_CAPS).canDirectPlay) return false;
  return audioTracksOf(item).length <= 1;
}

/** The audio-relative rendition to select for the chosen track, resolved from a
 * stable identity so a reordered track list still picks the right language. */
function renditionFor(item: MediaItem, audioIndex: number): number {
  const tracks = audioTracksOf(item);
  const want =
    tracks.find((t) => t.index === audioIndex) ?? tracks.find((t) => t.default) ?? tracks[0];
  if (!want) return 0;
  return resolveAudioRelativeIndex(tracks, audioTrackId(want));
}

/**
 * Play a media item on the TV: a plain compatible MP4 direct-plays in `<video>`;
 * everything else uses the complete-VOD HLS master. On Tizen the master runs
 * through native AVPlay (hardware AC3/EAC3/DTS surround + in-place audio switch,
 * the `tizen-avplay` engine); on webOS / for direct-play it uses `<video>` (+
 * hls.js). State is mirrored into React; resume + progress are persisted.
 */
export function useDirectPlayback(client: LumaClient, item: MediaItem): Playback {
  const videoRef = useRef<HTMLVideoElement>(null);
  const objectRef = useRef<HTMLObjectElement>(null);
  const engineRef = useRef<TvEngine | null>(null);
  const startedRef = useRef(false);

  const [verdict, setVerdict] = useState<DirectPlayVerdict | null>(null);
  const [error, setError] = useState<MessageKey | null>(null);
  const [terminated, setTerminated] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(true);
  const [ready, setReady] = useState(false);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(item.durationMs ? item.durationMs / 1000 : 0);
  const [bufEnd, setBufEnd] = useState(0);
  const [endedNonce, setEndedNonce] = useState(0);
  const [seekNonce, setSeekNonce] = useState(0);
  const [audioIndex, setAudioIndex] = useState(() => {
    const tracks = audioTracksOf(item);
    return (tracks.find((t) => t.default) ?? tracks[0])?.index ?? 0;
  });
  const audioIndexRef = useRef(audioIndex);
  audioIndexRef.current = audioIndex;

  const audioTracks = audioTracksOf(item);

  // Engine decision. A plain compatible MP4 direct-plays; otherwise the VOD
  // master, through native AVPlay on Tizen (surround passthrough + seamless audio)
  // and hls.js elsewhere (webOS MSE cannot decode AC3/EAC3, so it uses the AAC
  // master). selectEngine records the intent; tvDirectPlay is the runtime gate.
  const env = useMemo(detectTvEnv, []);
  const decision = selectEngine(item, env);
  const direct = decision.kind === 'direct' || tvDirectPlay(item);
  const useAvplay = !direct && env.platform === 'tizen' && avplayAvailable();
  const surface: 'video' | 'avplay' = useAvplay ? 'avplay' : 'video';
  const masterAac = masterNeedsAac(item, MSE_CAPS);
  const durationSec = item.durationMs ? item.durationMs / 1000 : 0;

  // Build + tear down the engine for this item. Audio switches do NOT re-create
  // it (they call setAudioRendition in place, below).
  useEffect(() => {
    setVerdict(canDirectPlay(item));
    setReady(false);
    startedRef.current = false;

    const listeners: EngineListeners = {
      onTime: setCur,
      onDuration: (s) => {
        if (s > 0) setDur(s);
      },
      onBuffered: setBufEnd,
      onPlay: () => {
        startedRef.current = true;
        setPlaying(true);
        setWaiting(false);
      },
      onPause: () => setPlaying(false),
      onWaiting: () => setWaiting(true),
      onPlaying: () => setWaiting(false),
      onEnded: () => setEndedNonce((n) => n + 1),
      onError: () => setError('player.cantPlay'),
      onReady: () => {
        setReady(true);
        // Ready-gated, resilient autoplay: retry until playback actually starts,
        // then stop so we never fight a real user pause.
        if (!startedRef.current) engineRef.current?.play();
      },
    };

    let engine: TvEngine | null = null;
    if (useAvplay) {
      engine = new AvplayEngine({
        client,
        item,
        durationSec,
        initialRendition: renditionFor(item, audioIndexRef.current),
        startSec: 0,
        listeners,
      });
    } else {
      const v = videoRef.current;
      if (!v) return;
      engine = new HtmlEngine({
        video: v,
        client,
        item,
        direct,
        masterAac,
        initialRendition: renditionFor(item, audioIndexRef.current),
        durationSec,
        startSec: 0,
        listeners,
      });
    }
    engineRef.current = engine;
    return () => {
      engineRef.current = null;
      engine?.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, item, useAvplay, direct, masterAac, durationSec]);

  // In-place audio rendition switch: no source reload, picture keeps playing.
  useEffect(() => {
    engineRef.current?.setAudioRendition(renditionFor(item, audioIndex));
  }, [item, audioIndex]);

  const getPosition = useCallback(() => engineRef.current?.position() ?? 0, []);
  const runtime = useCallback(() => engineRef.current?.duration() || durationSec, [durationSec]);

  // Resume + progress persistence, driven through the engine port.
  useResumeAndPersist(client, item, {
    getPosition,
    getDuration: runtime,
    seekTo: (s) => engineRef.current?.seekTo(s),
    ready,
    paused: !playing,
    endedNonce,
  });

  // Heartbeat the session for the admin dashboard + react to a remote termination.
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
    getPosition,
    getState: () => (playing ? 'playing' : 'paused'),
    pingSignal: playing,
    mode: direct ? 'direct' : 'transcode',
    player: 'LUMA TV',
    device: tvDevice(),
    eventsBaseUrl: client.baseUrl,
    idPrefix: 'tv',
    onTerminated: (message) => {
      engineRef.current?.pause();
      setTerminated(message.trim() || '');
    },
  });

  const togglePlay = useCallback(() => {
    const e = engineRef.current;
    if (!e) return;
    if (e.isPaused()) e.play();
    else e.pause();
  }, []);

  const clamp = useCallback(
    (target: number) => {
      const total = runtime();
      return Math.max(0, total > 0 ? Math.min(total - 0.5, target) : target);
    },
    [runtime],
  );

  const seekTo = useCallback(
    (absSec: number) => {
      engineRef.current?.seekTo(clamp(absSec));
      setSeekNonce((n) => n + 1);
    },
    [clamp],
  );

  const seek = useCallback((delta: number) => seekTo(getPosition() + delta), [seekTo, getPosition]);

  // ----- progressive (accelerating) seek --------------------------------------
  // While a direction key is held we accumulate a preview offset (the step grows
  // the longer you hold) and commit a SINGLE real seek the moment the key is
  // released. With the VOD master that seek is instant (no -ss respawn).
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const seekRef = useRef<{ target: number; step: number; last: number } | null>(null);

  const commitSeek = useCallback(() => {
    const s = seekRef.current;
    seekRef.current = null;
    setSeekPreview(null);
    if (s) seekTo(s.target);
  }, [seekTo]);

  const nudge = useCallback(
    (dir: -1 | 1) => {
      const total = runtime();
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
      let s = seekRef.current;
      if (!s || now - s.last > 700) {
        s = { target: getPosition(), step: 5, last: now };
      } else {
        s.step = Math.min(s.step * 1.4, 120);
      }
      const raw = s.target + dir * s.step;
      s.target = Math.max(0, total > 0 ? Math.min(total - 1, raw) : raw);
      s.last = now;
      seekRef.current = s;
      setSeekPreview(s.target);
    },
    [runtime, getPosition],
  );

  // Commit the pending seek the instant the user releases the key.
  useEffect(() => {
    const onKeyUp = () => {
      if (seekRef.current) commitSeek();
    };
    window.addEventListener('keyup', onKeyUp);
    return () => window.removeEventListener('keyup', onKeyUp);
  }, [commitSeek]);

  // Flush a pending seek if the player unmounts mid-gesture.
  useEffect(() => () => commitSeek(), [commitSeek]);

  const setAudio = useCallback(
    (index: number) => setAudioIndex((c) => (c === index ? c : index)),
    [],
  );

  return {
    videoRef,
    objectRef,
    surface,
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
    seekTo,
    getPosition,
    nudge,
    seekPreview,
    endedNonce,
    seekNonce,
  };
}
