import {
  type AudioTrack,
  audioTrackId,
  audioTrackLabel,
  audioTracksOf,
  avplayDirectPlayable,
  canDirectPlay,
  type DirectPlayVerdict,
  type KromaClient,
  type MediaItem,
  type MessageKey,
  NATIVE_TV_CAPS,
  type PlayEnv,
  resolveAudioRelativeIndex,
  selectEngine,
} from '@kroma/core';
import {
  type AudioFilterMode,
  type PlaneRect,
  storedAudioFilter,
  usePlaybackHeartbeat,
  useT,
} from '@kroma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  availableEngines,
  type EnginePref,
  getEnginePref,
  setEnginePref as persistEnginePref,
} from '#tv/app/enginePref';
import { AvplayEngine } from '#tv/features/playback/player/avplayEngine';
import {
  avplayAvailable,
  type EngineListeners,
  exoAvailable,
  getTauri,
  mpvAvailable,
  type TvEngine,
} from '#tv/features/playback/player/engine';
import { ExoEngine } from '#tv/features/playback/player/exoEngine';
import { HtmlEngine } from '#tv/features/playback/player/htmlEngine';
import { MpvEngine } from '#tv/features/playback/player/mpvEngine';
import { useResumeAndPersist } from '#tv/features/playback/player/useResumeAndPersist';
import { useSeekGesture } from '#tv/features/playback/player/useSeekGesture';

/** Which in-page surface (if any) an engine renders to. */
type Surface = 'video' | 'avplay' | 'mpv' | 'exo';

export interface Playback {
  /** The HTML `<video>` surface (HTML engine). Null while the AVPlay surface is used. */
  videoRef: React.RefObject<HTMLVideoElement | null>;
  /** The AVPlay `<object>` surface (native Tizen engine). */
  objectRef: React.RefObject<HTMLObjectElement | null>;
  /** Which surface to render. `mpv`/`exo` render nothing in-page (native plane behind). */
  surface: Surface;
  /** The active engine override (per-device pref); `auto` lets `planEngine` decide. */
  enginePref: EnginePref;
  /** Switch the engine live (persists + rebuilds at the current position). */
  setEngine: (p: EnginePref) => void;
  /** Resize the native video plane to a fraction-rect (or `null` = fullscreen). */
  setPlaneRect: (rect: PlaneRect | null) => void;
  /** Apply the audio filter / volume normalizer (§7) to the native engine (mpv /
   * ExoPlayer in place, AVPlay via the server's filtered remux). No-op on the
   * HTML engine - its in-page `<video>` is handled by the Web Audio graph. */
  setAudioFilter: (mode: AudioFilterMode) => void;
  /** Whether {@link setAudioFilter} actually reaches a DSP on this device, so
   * the chrome can hide the row rather than show a mode that does nothing
   * (API < 28 or audio passthrough on Android, a failed filtered remux on
   * AVPlay). Meaningless on the HTML engine, which uses Web Audio instead. */
  audioFilterSupported: boolean;
  verdict: DirectPlayVerdict | null;
  /** Codec/stream load failure, as an i18n key translated at the render site. */
  error: MessageKey | null;
  /** Admin-terminated message: a custom string, or '' for the default (the render
   * site supplies the localized fallback). Null while the session is live. */
  terminated: string | null;
  /** The engine is ready to play (a fresh frame is up) - reveal the native video plane. */
  ready: boolean;
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
 * @kroma/desktop shell is a Tauri app whose native mpv bridge is detectable). */
function detectTvEnv(): PlayEnv {
  if (mpvAvailable()) return { platform: 'desktop', safari: false }; // Linux shell -> mpv
  if (exoAvailable()) return { platform: 'androidtv', safari: false }; // Android TV shell -> ExoPlayer
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  // Tauri on macOS = WKWebView (Safari engine: native HEVC + AC3/EAC3), so treat it
  // as Safari web - caps + engine selection then match the in-page <video> we use
  // there, and no second (mpv) window is spawned.
  if (getTauri() != null && /Mac|Macintosh/i.test(ua)) return { platform: 'web', safari: true };
  const webos = /web0?s/i.test(ua) || (globalThis as Record<string, unknown>).webOS !== undefined;
  const chromeMajor = Number(/Chrome\/(\d+)/i.exec(ua)?.[1]);
  return {
    platform: webos ? 'webos' : 'tizen',
    safari: false,
    // Legacy webOS engines (Chromium < 99, pre-2024 models) cannot decode HEVC
    // through MSE/hls.js; their native media pipeline plays the HLS master
    // directly instead (same shape as Safari's native-HLS path).
    nativeHls: webos && Number.isFinite(chromeMajor) && chromeMajor < 99,
  };
}

/** The concrete backend to build for this item. */
type Engine = 'mpv' | 'exo' | 'avplay' | 'video-direct' | 'video-remux';

/** Resolve the backend from the user's engine preference, falling back to the
 * automatic decision. `auto` on Tizen keeps AVPlay (hardware surround), but the user
 * can force the HTML5 (`<video>` + hls.js) remux path instead; a manual choice that
 * isn't available on this platform (e.g. `mpv` off the Linux shell, `avplay` off
 * Tizen) quietly falls through to `auto`. */
function resolveEngine(pref: EnginePref, env: PlayEnv, autoDirect: boolean): Engine {
  // A stored engine no longer offered on this platform (e.g. a device left on
  // `remux` after it was retired on Android TV, where the WebView cannot decode
  // HEVC) must not strand playback on a dead engine - degrade it to `auto`.
  if (pref !== 'auto' && !availableEngines().includes(pref)) pref = 'auto';
  const tizenNative = env.platform === 'tizen' && avplayAvailable();
  // Manual overrides.
  if (pref === 'avplay' && tizenNative) return 'avplay';
  if (pref === 'webview') return 'video-direct';
  if (pref === 'remux') return 'video-remux';
  if (pref === 'mpv' && mpvAvailable()) return 'mpv';
  if (pref === 'exo' && exoAvailable()) return 'exo';
  // libVLC runs on the same native bridge as ExoPlayer (surface 'exo'); the
  // forceVlc flag (see planEngine) tells the bridge to software-decode from the start.
  if (pref === 'vlc' && exoAvailable()) return 'exo';
  // auto:
  if (tizenNative) return 'avplay';
  if (env.platform === 'desktop' && mpvAvailable()) return 'mpv';
  if (env.platform === 'androidtv' && exoAvailable()) return 'exo';
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

/** The resolved backend plan for an item: which engine + surface, the direct-play
 * flags, and the heartbeat playback mode. Pure (no React) so it stays out of the
 * hook body. */
interface EnginePlan {
  eng: Engine;
  surface: Surface;
  useMpv: boolean;
  useExo: boolean;
  useAvplay: boolean;
  avplayDirect: boolean;
  exoDirect: boolean;
  /** The user forced the "libVLC" engine: play every item through libVLC (software
   * decode) from the start, on the ExoPlayer bridge/surface. */
  forceVlc: boolean;
  direct: boolean;
  masterAac: boolean;
  playbackMode: 'direct' | 'remux' | 'transcode';
}

/** mpv / ExoPlayer / AVPlay render to their own plane behind the transparent UI,
 * so none of them uses an in-page media element. */
function surfaceFor(useMpv: boolean, useExo: boolean, useAvplay: boolean): Surface {
  if (useMpv) return 'mpv';
  if (useExo) return 'exo';
  if (useAvplay) return 'avplay';
  return 'video';
}

/** Video is always copied. AVPlay-direct plays the original file (direct);
 * AVPlay-master passes surround through (remux); only the hls.js AAC master
 * (webOS / MSE without AC3) re-encodes audio (transcode). */
function playbackModeFor(flags: {
  useMpv: boolean;
  useExo: boolean;
  useAvplay: boolean;
  exoDirect: boolean;
  avplayDirect: boolean;
  direct: boolean;
  aacMaster: boolean;
}): 'direct' | 'remux' | 'transcode' {
  const { useMpv, useExo, useAvplay, exoDirect, avplayDirect, direct, aacMaster } = flags;
  if (useMpv) return 'direct'; // mpv opens the original file (master only on fallback)
  if (useExo) return exoDirect ? 'direct' : 'remux';
  if (useAvplay) return avplayDirect ? 'direct' : 'remux';
  if (!direct) return aacMaster ? 'transcode' : 'remux';
  return 'direct';
}

/** Resolve the concrete backend decision for an item + environment + user pref. */
function planEngine(item: MediaItem, env: PlayEnv, pref: EnginePref): EnginePlan {
  const decision = selectEngine(item, env);
  const autoDirect = decision.kind === 'direct' || tvDirectPlay(item);
  // The user can override the automatic engine (profile menu -> Playback engine);
  // `auto` follows selectEngine.
  let eng = resolveEngine(pref, env, autoDirect);
  // A direct `<video>` on a container the webview can't demux (MKV/AVI in Safari)
  // would spin forever, so fall back to the server remux which repackages it.
  if (eng === 'video-direct' && !webviewCanDirectPlay(item)) eng = 'video-remux';
  const useMpv = eng === 'mpv';
  const useExo = eng === 'exo';
  const useAvplay = eng === 'avplay';
  // The user forced libVLC (runs on the exo bridge). It software-decodes ANY
  // codec, so it always opens the ORIGINAL file directly (no pointless server
  // remux), regardless of what the device's hardware decoders can handle.
  const forceVlc = useExo && pref === 'vlc';
  // ExoPlayer demuxes (at least) the same container set AVPlay does, so the same
  // gate decides whether it opens the ORIGINAL file (zero server work).
  const avplayDirect = useAvplay && avplayDirectPlayable(item);
  const exoDirect = useExo && (forceVlc || avplayDirectPlayable(item));
  const direct = eng === 'video-direct';
  return {
    eng,
    surface: surfaceFor(useMpv, useExo, useAvplay),
    useMpv,
    useExo,
    useAvplay,
    avplayDirect,
    exoDirect,
    forceVlc,
    direct,
    // Env-aware: Safari's native HLS decodes AC3/E-AC3 so its master is stream-copied
    // (5.1 kept); Chromium/webOS MSE can't, so `selectEngine` marks those AAC.
    masterAac: decision.aacMaster,
    playbackMode: playbackModeFor({
      useMpv,
      useExo,
      useAvplay,
      exoDirect,
      avplayDirect,
      direct,
      aacMaster: decision.aacMaster,
    }),
  };
}

/** Build the concrete backend for a resolved plan. Returns `null` only when the
 * in-page `<video>` surface isn't mounted yet (the caller retries next render); the
 * native-plane engines are always constructed. */
function createTvEngine(args: {
  eng: Engine;
  client: KromaClient;
  item: MediaItem;
  durationSec: number;
  rendition: number;
  startSec: number;
  exoDirect: boolean;
  avplayDirect: boolean;
  forceVlc: boolean;
  direct: boolean;
  masterAac: boolean;
  audioFilter: AudioFilterMode;
  forceNativeHls: boolean | undefined;
  video: HTMLVideoElement | null;
  listeners: EngineListeners;
}): TvEngine | null {
  const {
    eng,
    client,
    item,
    durationSec,
    rendition,
    startSec,
    exoDirect,
    avplayDirect,
    forceVlc,
    direct,
    masterAac: aacMaster,
    audioFilter,
    forceNativeHls,
    video,
    listeners,
  } = args;
  if (eng === 'mpv') {
    // Native mpv opens the original file directly (VA-API decode); an internal
    // direct->master fallback covers the rare file it cannot demux.
    const engine = new MpvEngine({
      client,
      item,
      durationSec,
      initialRendition: rendition,
      startSec,
      direct: true,
      audioFilter,
      listeners,
    });
    engine.start(); // async subscribe/open kept out of the constructor
    return engine;
  }
  if (eng === 'exo') {
    // Native ExoPlayer opens the original file directly (hardware decode); an
    // internal direct->master fallback covers the rare file it cannot open.
    // `forceVlc` makes libVLC the primary player (software-decode every codec).
    return new ExoEngine({
      client,
      item,
      durationSec,
      initialRendition: rendition,
      startSec,
      direct: exoDirect,
      forceVlc,
      audioFilter,
      listeners,
    });
  }
  if (eng === 'avplay') {
    return new AvplayEngine({
      client,
      item,
      durationSec,
      initialRendition: rendition,
      startSec,
      direct: avplayDirect,
      audioFilter,
      listeners,
    });
  }
  if (!video) return null;
  return new HtmlEngine({
    video,
    client,
    item,
    direct,
    masterAac: aacMaster,
    forceNativeHls,
    initialRendition: rendition,
    durationSec,
    startSec,
    listeners,
  });
}

/** A human label for the current TV device (admin dashboard). */
function tvDeviceLabel(useMpv: boolean, useExo: boolean): string {
  if (useMpv) return 'Desktop';
  if (useExo) return 'Android TV';
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent || '';
  if (/Tizen/i.test(ua)) return 'Samsung TV';
  if (/web0?s|LG/i.test(ua)) return 'LG TV';
  return 'TV';
}

/**
 * Play a media item on the TV: a plain compatible MP4 direct-plays in `<video>`;
 * everything else uses the complete-VOD HLS master. On Tizen the master runs
 * through native AVPlay (hardware AC3/EAC3/DTS surround + in-place audio switch,
 * the `tizen-avplay` engine); on webOS / for direct-play it uses `<video>` (+
 * hls.js). State is mirrored into React; resume + progress are persisted.
 */
export function useDirectPlayback(client: KromaClient, item: MediaItem): Playback {
  const t = useT();
  const videoRef = useRef<HTMLVideoElement>(null);
  const objectRef = useRef<HTMLObjectElement>(null);
  const engineRef = useRef<TvEngine | null>(null);
  const startedRef = useRef(false);

  const [error, setError] = useState<MessageKey | null>(null);
  const [terminated, setTerminated] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(true);
  const [ready, setReady] = useState(false);
  // Liveness heartbeat: bumped on every buffering signal so the load watchdog can
  // tell "slow but alive" (keep waiting) from "dead" (fail). See the watchdog effect.
  const [loadBeat, setLoadBeat] = useState(0);
  // Optimistic: a native plane is assumed to have DSP until its engine says
  // otherwise (the Android bridge only learns on the first real attempt, and
  // AVPlay only when the server's filtered remux fails).
  const [audioFilterSupported, setAudioFilterSupported] = useState(true);
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
  // Resolved each render (via `planEngine`) so a live engine-pref change (profile
  // menu -> Playback engine) takes effect on the next item build.
  const env = useMemo(detectTvEnv, []);
  // Engine override, reactive so an in-player switch (Settings -> "Moteur de
  // lecture") re-plans + rebuilds the engine live. Seeded from the per-device
  // pref (shared with the profile-menu picker).
  const [enginePref, setEnginePrefState] = useState<EnginePref>(getEnginePref);
  const {
    eng,
    surface,
    useMpv,
    useExo,
    avplayDirect,
    exoDirect,
    forceVlc,
    direct,
    masterAac,
    playbackMode,
  } = planEngine(item, env, enginePref);
  const durationSec = item.durationMs ? item.durationMs / 1000 : 0;
  // The runtime decode verdict for this item (from probed `capabilities()`). Drives
  // the pre-play warning and, when a `<video>`-engine attempt fails, the SPECIFIC
  // reason (e.g. "AV1 not supported on this device") instead of a generic cantPlay -
  // the server is remux-only, so an undecodable video codec here truly can't play
  // (mpv, with software dav1d, is the path for AV1 on a pre-M3 Mac).
  const playVerdict = useMemo(() => canDirectPlay(item), [item]);
  const failKey: MessageKey =
    surface === 'video' && !playVerdict.canDirectPlay ? playVerdict.messageKey : 'player.cantPlay';

  // Resolve the RESUME start position BEFORE building the engine, so it opens directly
  // THERE (HtmlEngine anchors the server ffmpeg at it, AVPlay/mpv open at that offset)
  // instead of loading at 0, going ready, then re-seeking - which reloads the whole
  // stream (a second ffmpeg session for the HLS path). `null` = not resolved yet; the
  // engine build waits for it (a fast progress fetch, gone before the first frame).
  // Keyed by item id: on an in-place item change (up-next autoplay) `resolved` still holds
  // the PREVIOUS item's value for one render, so the engine build must gate on the id (via
  // `startSec` below) or it would build at the wrong offset for a render then rebuild.
  const [resolved, setResolved] = useState<{ id: string; sec: number } | null>(null);
  useEffect(() => {
    if (!client.hasAuth) {
      setResolved({ id: item.id, sec: 0 });
      return;
    }
    let done = false;
    const settle = (sec: number) => {
      if (done) return;
      done = true;
      setResolved({ id: item.id, sec });
    };
    // Never let a stalled progress fetch block playback forever - fall back to start at 0.
    const timer = setTimeout(() => settle(0), 4000);
    client
      .itemProgress(item.id)
      .then((p) => {
        const durMs = p?.durationMs ?? item.durationMs ?? 0;
        const posSec = p ? p.positionMs / 1000 : 0;
        // Resume only if meaningfully into the title and not ~finished (else start at 0).
        settle(p && posSec > 15 && (!durMs || p.positionMs < durMs * 0.95) ? posSec : 0);
      })
      .catch(() => settle(0));
    return () => {
      done = true;
      clearTimeout(timer);
    };
  }, [client, item]);
  // Valid ONLY for the current item - null while this item's resume is still resolving.
  const startSec = resolved?.id === item.id ? resolved.sec : null;

  // Build + tear down the engine for this item. Audio switches do NOT re-create
  // it (they call setAudioRendition in place, below).
  // biome-ignore lint/correctness/useExhaustiveDependencies: env.nativeHls is a session-constant capability; the dep list is intentionally curated to rebuild only on item/engine changes.
  useEffect(() => {
    setReady(false);
    startedRef.current = false;
    if (startSec == null) return; // wait until the resume position is known
    // Show the resume position on the scrubber IMMEDIATELY (the stream opens there), so
    // the cursor doesn't sit at 0:00 while loading and then teleport once ready.
    setCur(startSec);

    const listeners: EngineListeners = {
      onTime: (s) => {
        // Before playback has really started, ignore positions before the resume point
        // so the scrubber stays put instead of dipping to 0 and jumping back once the
        // seek lands (the engine can briefly report 0 during the initial open).
        if (!startedRef.current && startSec != null && s < startSec - 2) return;
        setCur(s);
      },
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
      onWaiting: () => {
        setWaiting(true);
        // Proof of life for the load watchdog: a slowly-opening native plane
        // (libVLC software-decoding 10-bit) keeps emitting buffering signals, so
        // resetting the timer on each one lets it take as long as it needs while a
        // truly dead load (no signals) still fails after the grace period.
        setLoadBeat((n) => n + 1);
      },
      onPlaying: () => setWaiting(false),
      onEnded: () => setEndedNonce((n) => n + 1),
      onError: () => setError(failKey),
      onAudioFilterUnavailable: () => setAudioFilterSupported(false),
      onReady: () => {
        setReady(true);
        // A load that finally signals ready IS working - clear any premature error
        // the load watchdog raised (libVLC software-decoding 10-bit HEVC at a deep
        // resume point can take longer than the grace period to reach ready, but it
        // recovers). Without this the "codec not supported" toast lingers over a
        // playing video.
        setError(null);
        // Ready-gated, resilient autoplay: retry until playback actually starts,
        // then stop so we never fight a real user pause.
        if (!startedRef.current) engineRef.current?.play();
      },
    };

    const engine = createTvEngine({
      eng,
      client,
      item,
      durationSec,
      rendition: renditionFor(item, audioIndexRef.current),
      startSec,
      exoDirect,
      avplayDirect,
      forceVlc,
      direct,
      masterAac,
      // Persisted mode, read at build time so a remembered filter is active from
      // the first frame (AVPlay even picks its source from it); later changes
      // arrive in place through setAudioFilter.
      audioFilter: storedAudioFilter(),
      forceNativeHls: env.nativeHls,
      video: videoRef.current,
      listeners,
    });
    if (!engine) return; // <video> surface not mounted yet; rebuild next render
    engineRef.current = engine;
    // Seed from whatever the backend can answer upfront (the Android bridge
    // knows its API level immediately); the rest arrives via the listener.
    setAudioFilterSupported(engine.audioFilterSupported?.() ?? true);
    return () => {
      engineRef.current = null;
      engine.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    client,
    item,
    eng,
    exoDirect,
    avplayDirect,
    direct,
    masterAac,
    durationSec,
    startSec,
    failKey,
  ]);

  // In-place audio rendition switch: no source reload, picture keeps playing.
  useEffect(() => {
    engineRef.current?.setAudioRendition(renditionFor(item, audioIndex));
  }, [item, audioIndex]);

  // Safety net + diagnostic: a load that never signals ready would otherwise spin
  // forever. `<video>`: blocked http media, macOS ATS, an undecodable codec. mpv:
  // a socket that stops responding mid-session (hard startup failures already fail
  // fast via mpv://error + the engine's status probe). AVPlay reports its own
  // prepare errors. After a grace period, log what we know and surface the failure.
  useEffect(() => {
    if (surface === 'avplay' || ready || error) return;
    // The native software-decode planes (ExoPlayer's libVLC fallback, mpv) can take
    // much longer than a hardware/`<video>` load to reach ready - libVLC decoding
    // 10-bit HEVC at a deep resume point over the LAN routinely needs 20s+ - so give
    // them a longer grace before crying failure, or the video plays UNDER a false
    // "codec not supported" toast. `loadBeat` is in the deps: each buffering signal
    // re-arms this timer, so a slow-but-alive load never trips it (and a dead one,
    // which emits nothing, still fails after the grace).
    const graceMs = surface === 'exo' || surface === 'mpv' ? 30000 : 15000;
    const graceS = graceMs / 1000;
    const id = window.setTimeout(() => {
      if (surface === 'mpv' || surface === 'exo') {
        console.error(`[KROMA] ${surface} engine did not signal ready in ${graceS}s`);
      } else {
        const v = videoRef.current;
        const e = v?.error;
        console.error(
          `[KROMA] stream did not load in ${graceS}s: networkState=${v?.networkState} ` +
            `readyState=${v?.readyState} errorCode=${e?.code ?? '-'} ${e?.message ?? ''} ` +
            `src=${v?.currentSrc || v?.src || '(none)'}`,
        );
      }
      setError(failKey);
    }, graceMs);
    return () => window.clearTimeout(id);
  }, [surface, ready, error, failKey, loadBeat]);

  const getPosition = useCallback(() => engineRef.current?.position() ?? 0, []);
  // Resize the native video plane (shrink into the settings card, or null =
  // fullscreen). No-op on the HTML engine (no setRect: the chrome CSS-transforms
  // the <video> instead).
  const setPlaneRect = useCallback((rect: PlaneRect | null) => {
    engineRef.current?.setRect?.(rect);
  }, []);
  // Push an audio-filter change into the engine (native planes only implement
  // it; the HTML engine leaves it to the Web Audio graph, so this is a no-op).
  const setAudioFilter = useCallback((mode: AudioFilterMode) => {
    engineRef.current?.setAudioFilter?.(mode);
  }, []);
  const runtime = useCallback(() => engineRef.current?.duration() || durationSec, [durationSec]);

  // Resume + progress persistence, driven through the engine port.
  useResumeAndPersist(client, item, {
    getPosition,
    getDuration: runtime,
    paused: !playing,
    endedNonce,
  });

  // Heartbeat the session for the admin dashboard + react to a remote termination.
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
    getAudio: () =>
      audioTrackLabel(
        t,
        audioTracks.find((a) => a.index === audioIndex),
      ),
    // Ping promptly on play/pause, buffering, and audio-track changes.
    pingSignal: `${playing}|${waiting}|${audioIndex}`,
    mode: playbackMode,
    player: 'KROMA TV',
    device: tvDeviceLabel(useMpv, useExo),
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

  // Live engine switch (Settings -> "Moteur de lecture"): persist the choice, then
  // re-plan with it AND reset the resume anchor to the CURRENT position so the
  // rebuilt engine resumes here instead of jumping back to the start.
  const setEngine = useCallback(
    (p: EnginePref) => {
      if (p === enginePref) return;
      const pos = engineRef.current?.position() ?? 0;
      persistEnginePref(p);
      setResolved({ id: item.id, sec: pos });
      setEnginePrefState(p);
    },
    [enginePref, item.id],
  );

  return {
    videoRef,
    objectRef,
    surface,
    enginePref,
    setEngine,
    setPlaneRect,
    setAudioFilter,
    audioFilterSupported,
    verdict: playVerdict,
    error,
    terminated,
    ready,
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
