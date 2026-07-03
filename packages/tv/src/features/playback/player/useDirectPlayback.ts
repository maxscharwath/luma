import {
  type AudioTrack,
  audioTrackId,
  audioTrackLabel,
  audioTracksOf,
  avplayDirectPlayable,
  canDirectPlay,
  type DirectPlayVerdict,
  type LumaClient,
  type MediaItem,
  type MessageKey,
  NATIVE_TV_CAPS,
  type PlayEnv,
  resolveAudioRelativeIndex,
  selectEngine,
} from '@luma/core';
import { usePlaybackHeartbeat, useT } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AvplayEngine } from '#tv/features/playback/player/avplayEngine';
import {
  avplayAvailable,
  type EngineListeners,
  getTauri,
  mpvAvailable,
  type TvEngine,
} from '#tv/features/playback/player/engine';
import { HtmlEngine } from '#tv/features/playback/player/htmlEngine';
import { MpvEngine } from '#tv/features/playback/player/mpvEngine';
import { useResumeAndPersist } from '#tv/features/playback/player/useResumeAndPersist';
import { useSeekGesture } from '#tv/features/playback/player/useSeekGesture';
import { type EnginePref, getEnginePref } from '#tv/app/enginePref';

export interface Playback {
  /** The HTML `<video>` surface (HTML engine). Null while the AVPlay surface is used. */
  videoRef: React.RefObject<HTMLVideoElement>;
  /** The AVPlay `<object>` surface (native Tizen engine). */
  objectRef: React.RefObject<HTMLObjectElement>;
  /** Which surface to render. `mpv` renders nothing in-page (native window behind). */
  surface: 'video' | 'avplay' | 'mpv';
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
  /** Begin a directional seek press (remote key / mouse button down). A short press
   * is a stacking tap; held past a threshold it becomes an accelerating scrub. */
  seekPress: (dir: -1 | 1) => void;
  /** A discrete directional tap (OK on a focused rewind/forward control). */
  seekTap: (dir: -1 | 1) => void;
  /** Live-preview an absolute position while clicking / dragging the scrub bar. */
  seekScrub: (absSec: number) => void;
  /** Commit the current scrub preview (drag release / bar click). */
  seekScrubCommit: () => void;
  /** Pending absolute position (s) during a seek gesture, else `null`. */
  seekPreview: number | null;
  /** Increments each time playback reaches the end (drives up-next autoplay). */
  endedNonce: number;
  /** Increments on every committed seek (re-anchors the subtitle cue pointer). */
  seekNonce: number;
}

/** The browser/platform environment for engine selection (TVs are Chromium; the
 * @luma/desktop shell is a Tauri app whose native mpv bridge is detectable). */
function detectTvEnv(): PlayEnv {
  if (mpvAvailable()) return { platform: 'desktop', safari: false }; // Linux shell -> mpv
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  // Tauri on macOS = WKWebView (Safari engine: native HEVC + AC3/EAC3), so treat it
  // as Safari web - caps + engine selection then match the in-page <video> we use
  // there, and no second (mpv) window is spawned.
  if (getTauri() != null && /Mac|Macintosh/i.test(ua)) return { platform: 'web', safari: true };
  const webos =
    /web0?s/i.test(ua) || typeof (globalThis as Record<string, unknown>).webOS !== 'undefined';
  return { platform: webos ? 'webos' : 'tizen', safari: false };
}

/** The concrete backend to build for this item. */
type Engine = 'mpv' | 'avplay' | 'video-direct' | 'video-remux';

/** Resolve the backend from the user's engine preference, falling back to the
 * automatic decision. `auto` on Tizen keeps AVPlay (hardware surround), but the user
 * can force the HTML5 (`<video>` + hls.js) remux path instead; a manual choice that
 * isn't available on this platform (e.g. `mpv` off the Linux shell, `avplay` off
 * Tizen) quietly falls through to `auto`. */
function resolveEngine(
  pref: EnginePref,
  env: PlayEnv,
  autoDirect: boolean,
): Engine {
  const tizenNative = env.platform === 'tizen' && avplayAvailable();
  // Manual overrides.
  if (pref === 'avplay' && tizenNative) return 'avplay';
  if (pref === 'webview') return 'video-direct';
  if (pref === 'remux') return 'video-remux';
  if (pref === 'mpv' && mpvAvailable()) return 'mpv';
  // auto:
  if (tizenNative) return 'avplay';
  if (env.platform === 'desktop' && mpvAvailable()) return 'mpv';
  return autoDirect ? 'video-direct' : 'video-remux';
}

/** A plain, single-audio MP4 a bare TV `<video>` direct-plays natively. */
function tvDirectPlay(item: MediaItem): boolean {
  const container = (item.container ?? '').toLowerCase();
  if (container !== 'mp4' && container !== 'mov' && container !== 'm4v') return false;
  if (!canDirectPlay(item, NATIVE_TV_CAPS).canDirectPlay) return false;
  return audioTracksOf(item).length <= 1;
}

/** Container MIME the webview needs to demux a bare `<video src>`. */
const CONTAINER_MIME: Record<string, string> = {
  mp4: 'video/mp4',
  mov: 'video/mp4',
  m4v: 'video/mp4',
  webm: 'video/webm',
};

/** Whether the webview can demux this item's container for a direct `<video src>`.
 * Safari / WKWebView has no Matroska (MKV) or AVI demuxer, so a forced direct-play
 * on one loads forever at HAVE_NOTHING with no error - callers fall back to the
 * server remux (which repackages it into a webview-playable stream) instead. */
function webviewCanDirectPlay(item: MediaItem): boolean {
  if (typeof document === 'undefined') return true;
  const mime = CONTAINER_MIME[(item.container ?? '').toLowerCase()];
  if (!mime) return false;
  return document.createElement('video').canPlayType(mime) !== '';
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
  const t = useT();
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

  // Engine decision. On Tizen EVERYTHING goes through native AVPlay (hardware
  // plane, surround passthrough): the engine opens the ORIGINAL file directly
  // when avplayDirectPlayable holds (zero server work no ffmpeg session at
  // all; native seeks + in-place audio switching) and the stream-copy master
  // otherwise, with an internal direct→master error fallback. webOS / no-AVPlay
  // runtimes use `<video>`: direct for a plain compatible MP4, else hls.js on
  // the AAC master (webOS MSE cannot decode AC3/EAC3).
  const env = useMemo(detectTvEnv, []);
  const decision = selectEngine(item, env);
  const autoDirect = decision.kind === 'direct' || tvDirectPlay(item);
  // The user can override the automatic engine (profile menu → Playback engine);
  // `auto` follows selectEngine.
  let eng = resolveEngine(getEnginePref(), env, autoDirect);
  // A direct `<video>` on a container the webview can't demux (MKV/AVI in Safari)
  // would spin forever, so fall back to the server remux which repackages it.
  if (eng === 'video-direct' && !webviewCanDirectPlay(item)) eng = 'video-remux';
  const useMpv = eng === 'mpv';
  const useAvplay = eng === 'avplay';
  const avplayDirect = useAvplay && avplayDirectPlayable(item);
  const direct = eng === 'video-direct';
  // mpv / AVPlay render to their own plane behind the transparent UI, so neither
  // uses an in-page media element.
  const surface: 'video' | 'avplay' | 'mpv' = useMpv ? 'mpv' : useAvplay ? 'avplay' : 'video';
  // Env-aware: Safari's native HLS decodes AC3/E-AC3 so its master is stream-copied
  // (5.1 kept); Chromium/webOS MSE can't, so `selectEngine` marks those AAC.
  const masterAac = decision.aacMaster;
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
    if (useMpv) {
      // Native mpv opens the original file directly (VA-API decode); an internal
      // direct→master fallback covers the rare file it cannot demux.
      engine = new MpvEngine({
        client,
        item,
        durationSec,
        initialRendition: renditionFor(item, audioIndexRef.current),
        startSec: 0,
        direct: true,
        listeners,
      });
    } else if (useAvplay) {
      engine = new AvplayEngine({
        client,
        item,
        durationSec,
        initialRendition: renditionFor(item, audioIndexRef.current),
        startSec: 0,
        direct: avplayDirect,
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
  }, [client, item, useMpv, useAvplay, avplayDirect, direct, masterAac, durationSec]);

  // In-place audio rendition switch: no source reload, picture keeps playing.
  useEffect(() => {
    engineRef.current?.setAudioRendition(renditionFor(item, audioIndex));
  }, [item, audioIndex]);

  // Safety net + diagnostic: a `<video>`/HLS load that never signals ready (blocked
  // http media, macOS ATS, an undecodable codec) would otherwise spin forever.
  // After a grace period, log the element's exact state and surface the failure.
  useEffect(() => {
    if (surface !== 'video' || ready || error) return;
    const id = window.setTimeout(() => {
      const v = videoRef.current;
      const e = v?.error;
      console.error(
        `[LUMA] stream did not load in 15s: networkState=${v?.networkState} ` +
          `readyState=${v?.readyState} errorCode=${e?.code ?? '-'} ${e?.message ?? ''} ` +
          `src=${v?.currentSrc || v?.src || '(none)'}`,
      );
      setError('player.cantPlay');
    }, 15000);
    return () => window.clearTimeout(id);
  }, [surface, ready, error]);

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
    if (useMpv) return 'Desktop';
    const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent || '';
    if (/Tizen/i.test(ua)) return 'Samsung TV';
    if (/web0?s|LG/i.test(ua)) return 'LG TV';
    return 'TV';
  };
  // Video is always copied. AVPlay-direct plays the original file (direct);
  // AVPlay-master passes surround through (remux); only the hls.js AAC master
  // (webOS / MSE without AC3) re-encodes audio (transcode).
  let playbackMode: 'direct' | 'remux' | 'transcode' = 'direct';
  if (useMpv) playbackMode = 'direct'; // mpv opens the original file (master only on fallback)
  else if (useAvplay) playbackMode = avplayDirect ? 'direct' : 'remux';
  else if (!direct) playbackMode = masterAac ? 'transcode' : 'remux';
  usePlaybackHeartbeat({
    client,
    enabled: client.hasAuth,
    itemId: item.id,
    durationMs: item.durationMs ?? null,
    getPosition,
    getState: () => {
      if (!playing) return 'paused';
      return waiting ? 'buffering' : 'playing';
    },
    getAudio: () => audioTrackLabel(t, audioTracks.find((a) => a.index === audioIndex)),
    // Ping promptly on play/pause, buffering, and audio-track changes.
    pingSignal: `${playing}|${waiting}|${audioIndex}`,
    mode: playbackMode,
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

  // Tap-vs-hold seek gesture shared by the remote and the mouse: a short press
  // stacks fixed 5s taps into one commit (precise), a held press turns into an
  // accelerating scrub (fast), and `scrub` drives the same preview from an
  // absolute position for a mouse click / drag on the bar. Only ONE real seek per
  // gesture; with the VOD master / direct source that seek is instant.
  const {
    preview: seekPreview,
    press: seekPress,
    tap: seekTap,
    scrub: seekScrub,
    commit: seekScrubCommit,
  } = useSeekGesture({ getPosition, duration: runtime, seekTo });

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
    seekPress,
    seekTap,
    seekScrub,
    seekScrubCommit,
    seekPreview,
    endedNonce,
    seekNonce,
  };
}
