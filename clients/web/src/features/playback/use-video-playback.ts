import {
  audioTracksOf,
  capabilities,
  type EngineDecision,
  MSE_CAPS,
  masterNeedsAac,
  type PlayEnv,
  SAFARI_CAPS,
  selectEngine,
} from '@kroma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  getWebEnginePref,
  setWebEnginePref,
  type WebEnginePref,
} from '#web/features/playback/engine-pref';
import { preferredAudioIndex } from '#web/features/playback/track-prefs';
import {
  attachMediaSource,
  bindMediaEvents,
  type VideoPlayback,
} from '#web/features/playback/video-engine';
import { kromaClient, type MovieView } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

// The media-element / hls / track-wiring engine lives in `./videoEngine`; the
// `VideoPlayback` shape is re-exported so call sites keep importing it here.
export type { VideoPlayback } from '#web/features/playback/video-engine';

/** Detect the browser environment for engine selection. Safari (and iOS) use
 * native HLS (and decode AC3/EAC3), so they get the stream-copy master; other
 * browsers go through hls.js (MSE) with the AAC master when needed. The runtime
 * caps (canPlayType/MediaSource probes) widen direct-play to whatever THIS
 * browser actually hardware-decodes (e.g. HEVC MP4s on Chrome 107+). */
function detectWebEnv(): PlayEnv {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  const safari =
    /^((?!chrome|chromium|android|crios|fxios|edg).)*safari/i.test(ua) ||
    /iP(ad|hone|od)/i.test(ua);
  return { platform: 'web', safari, runtimeCaps: capabilities() };
}

/**
 * Owns the `<video>` element: playback state (time/duration/buffer/volume/rate),
 * the source decision (direct-play `<video src>` vs the continuous HLS remux
 * master), fullscreen tracking, and every transport action. The HLS stream is
 * anchored at `anchor` (input -ss) and its clock is relative, so the hook reports
 * the absolute position as `baseSec + currentTime`; a seek inside the produced
 * range is native, otherwise it re-anchors (remounts at the target). Capability
 * detection needs the DOM, so the source is resolved post-mount.
 */
export function useVideoPlayback(item: MovieView): VideoPlayback {
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const barRef = useRef<HTMLDivElement>(null);

  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [ready, setReady] = useState(false);
  const [cur, setCur] = useState(0);
  const [dur, setDur] = useState(item.durationMs ? item.durationMs / 1000 : 0);
  const [bufEnd, setBufEnd] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [rate, setRate] = useState(1);
  const [fs, setFs] = useState(false);
  const [useHls, setUseHls] = useState(false);
  const [audioIndex, setAudioIndex] = useState(() => {
    const tracks = audioTracksOf(item);
    return (tracks.find((t) => t.default) ?? tracks[0])?.index ?? 0;
  });
  // The HLS remux anchor (s): the master is started at `?t=anchor` (input -ss).
  // hls.js reports time RELATIVE to the anchor, so the absolute position is
  // `anchor + currentTime` (see `baseSec`). A resume / far seek / backward seek
  // changes the anchor, which REMOUNTS the <video> (keyed by anchor) for a clean
  // fresh attach. `bootAnchor === null` means "resume not resolved yet": the
  // source effect waits so the FIRST attach is already at the resume position
  // (instead of attaching at 0 then re-anchoring).
  const { client, user } = useAuth();
  const [anchor, setAnchor] = useState(0);
  const [bootAnchor, setBootAnchor] = useState<number | null>(null);
  useEffect(() => {
    setBootAnchor(null);
    if (!user) {
      setAnchor(0);
      setBootAnchor(0);
      return;
    }
    let cancelled = false;
    client
      .itemProgress(item.id)
      .then((p) => {
        if (cancelled) return;
        const durMs = p?.durationMs ?? item.durationMs ?? 0;
        const posSec = (p?.positionMs ?? 0) / 1000;
        const resume = p && posSec > 15 && (!durMs || p.positionMs < durMs * 0.95) ? posSec : 0;
        setAnchor(resume);
        setBootAnchor(resume);
      })
      .catch(() => {
        if (!cancelled) {
          setAnchor(0);
          setBootAnchor(0);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, user, item.id, item.durationMs]);
  const [hover, setHover] = useState<{ x: number; t: number; w: number } | null>(null);
  const [scrubbing, setScrubbing] = useState(false);
  // While dragging the scrub bar, the previewed absolute position (s): the thumb
  // follows it but we only COMMIT the seek on release.
  const [scrubPreview, setScrubPreview] = useState<number | null>(null);
  const scrubPreviewRef = useRef<number | null>(null);
  scrubPreviewRef.current = scrubPreview;
  const audioIndexRef = useRef(0);
  audioIndexRef.current = audioIndex;

  const audioTracks = audioTracksOf(item);

  // Honour the account's preferred audio language for the *initial* track pick.
  // The web session hydrates asynchronously, so this can't live in the
  // `audioIndex` initialiser it runs once `user` is known, before the source
  // attaches (the source effect waits on `bootAnchor`, resolved even later), and
  // uses the raw setter so it does NOT re-anchor like a manual `setAudio` switch.
  const audioPrefApplied = useRef(false);
  useEffect(() => {
    if (audioPrefApplied.current || !user) return;
    audioPrefApplied.current = true;
    const idx = preferredAudioIndex(audioTracks, user.audioLanguage);
    if (idx != null) setAudioIndex(idx);
  }, [user, audioTracks]);

  const env = useMemo(detectWebEnv, []);
  // `forceHls` is the direct-play safety net: if a bare `<video src>` errors
  // (an over-optimistic capability probe, a quirky file), we drop to the HLS
  // master at the same position instead of dying with a black screen.
  const [forceHls, setForceHls] = useState(false);
  // Manual engine override (Settings → "Moteur de lecture"). `remux` and `shaka`
  // behave like the direct-play safety net (always the HLS master), differing only
  // in the MSE engine that plays it (hls.js vs Shaka Player); `direct` forces the
  // bare `<video src>` (still guarded by the decode-error fallback below); `auto`
  // defers to the runtime-cap decision.
  const [enginePref, setEnginePrefState] = useState<WebEnginePref>(getWebEnginePref);
  const decision = useMemo<EngineDecision>(() => {
    if (forceHls || enginePref === 'remux' || enginePref === 'shaka') {
      return {
        kind: 'web-mse',
        aacMaster: masterNeedsAac(item, env.safari ? SAFARI_CAPS : MSE_CAPS),
      };
    }
    if (enginePref === 'direct') return { kind: 'direct', aacMaster: false };
    return selectEngine(item, env);
  }, [item, env, forceHls, enginePref]);
  const hlsRef = useRef<import('hls.js').default | null>(null);
  const shakaRef = useRef<import('#web/features/playback/video-engine').ShakaPlayerLike | null>(
    null,
  );

  // The absolute-position offset: `absolute = baseSec + video.currentTime`. For
  // HLS, `-noaccurate_seek` starts the stream at the keyframe AT-OR-BEFORE the
  // anchor (so video + audio begin together), which is usually a bit before the
  // requested anchor. The SERVER reports that real start via the `X-Hls-Start`
  // header; we read it BEFORE attaching so the clock + subtitles line up with the
  // A/V. Direct-play is already absolute (0). `srcReady` gates the attach until
  // the offset is known.
  const [baseSec, setBaseSec] = useState(0);
  const [srcReady, setSrcReady] = useState(false);
  // The server's TRUE media duration (s) from the `X-Media-Duration` header, used
  // when the catalog row was never probed (so `item.durationMs` is missing). 0 =
  // not (yet) known. Without it the growing HLS EVENT playlist's live edge would
  // be all the player could show as the total (a "tiny" movie).
  const [serverDurSec, setServerDurSec] = useState(0);
  useEffect(() => {
    if (bootAnchor === null) return; // wait until resume has picked the anchor
    setSrcReady(false);
    if (decision.kind === 'direct') {
      setBaseSec(0);
      setSrcReady(true);
      return;
    }
    let cancelled = false;
    const url = kromaClient().hlsMasterUrl(item.id, decision.aacMaster, anchor, audioIndex);
    fetch(url)
      .then((r) => {
        const k = Number(r.headers.get('X-Hls-Start'));
        const d = Number(r.headers.get('X-Media-Duration'));
        if (!cancelled) {
          setBaseSec(Number.isFinite(k) ? k : anchor);
          if (Number.isFinite(d) && d > 0) setServerDurSec(d);
          setSrcReady(true);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setBaseSec(anchor);
          setSrcReady(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [item.id, decision, anchor, audioIndex, bootAnchor]);

  // Authoritative total length (ms): the catalog runtime, else the server header.
  // Everything that needs the full timeline (the slider, seek clamps, the media
  // -event duration binding) reads this so an unprobed file still spans the whole
  // movie instead of the HLS live edge.
  const knownDurationMs =
    item.durationMs || (serverDurSec > 0 ? Math.round(serverDurSec * 1000) : 0);
  useEffect(() => {
    if (knownDurationMs > 0) setDur(knownDurationMs / 1000);
  }, [knownDurationMs]);

  // ----- video element wiring -------------------------------------------------
  // Re-binds on anchor/audio change too: those REMOUNT the <video> (keyed by
  // anchor+audio in the parent), so this must rebind to the fresh element.
  // biome-ignore lint/correctness/useExhaustiveDependencies: rebind on remount.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    return bindMediaEvents(
      v,
      item,
      {
        setCur,
        setDur,
        setBufEnd,
        setPlaying,
        setWaiting,
        setVolume,
        setMuted,
        setRate,
        setReady,
      },
      baseSec,
      knownDurationMs,
    );
  }, [item, anchor, audioIndex, baseSec, knownDurationMs]);

  // ----- source wiring --------------------------------------------------------
  // Attaches the source. The chosen audio (`audioIndex`) is MUXED into the stream
  // (in the URL), so a language change remounts the element with the new audio -
  // there is no in-place rendition switch.
  useEffect(() => {
    const v = videoRef.current;
    // Wait until resume picked the anchor AND the real start (baseSec) is known.
    if (!v || bootAnchor === null || !srcReady) return;
    // Shaka is the DEFAULT MSE engine for the HLS master (it handles our streams
    // best); hls.js is used only on the explicit `remux` override. Safari keeps its
    // native HLS pipeline (surround via stream-copy) unless the user picks Shaka.
    const safariNative = env.safari && enginePref !== 'shaka';
    return attachMediaSource({
      v,
      item,
      decision,
      useNativeHls: safariNative,
      useShaka: !safariNative && enginePref !== 'remux',
      startSec: anchor,
      audioRel: audioIndex,
      hlsRef,
      shakaRef,
      setUseHls,
      setReady,
    });
  }, [item, decision, env.safari, enginePref, anchor, audioIndex, bootAnchor, srcReady]);

  useEffect(() => {
    const onFs = () => setFs(Boolean(document.fullscreenElement));
    document.addEventListener('fullscreenchange', onFs);
    return () => document.removeEventListener('fullscreenchange', onFs);
  }, []);

  // Direct-play error fallback: a media error on the bare `<video src>` swaps
  // to the HLS master anchored at the position we died at.
  // biome-ignore lint/correctness/useExhaustiveDependencies: rebind on remount.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || decision.kind !== 'direct') return;
    const onErr = () => {
      setAnchor(Math.max(0, Math.floor(v.currentTime)));
      setForceHls(true);
    };
    v.addEventListener('error', onErr);
    return () => v.removeEventListener('error', onErr);
  }, [decision.kind, item.id, anchor, audioIndex]);

  // A new item starts from a fresh decision.
  // biome-ignore lint/correctness/useExhaustiveDependencies: item.id is an intentional trigger (not referenced in the effect); reset forceHls whenever the item changes, not on every render.
  useEffect(() => setForceHls(false), [item.id]);

  // ----- actions --------------------------------------------------------------
  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) {
      const p = v.play();
      if (typeof p?.then === 'function') p.catch(() => undefined);
    } else v.pause();
  }, []);

  // Seek to an ABSOLUTE position (seconds). If the target lies inside what the
  // current anchored stream has produced (its relative seekable range), it is an
  // instant native seek. Otherwise (seeking BEFORE the anchor, or PAST the
  // produced edge) we re-anchor: `setAnchor(target)` remounts the <video> with a
  // fresh remux started at `target`, available in ~1s. Either way the slider is
  // correct (absolute = anchor + relative).
  const seekTo = useCallback(
    (absSec: number) => {
      const v = videoRef.current;
      if (!v) return;
      const total = knownDurationMs ? knownDurationMs / 1000 : 0;
      const target = Math.max(0, total ? Math.min(total - 1, absSec) : absSec);

      if (decision.kind === 'direct') {
        v.currentTime = target; // direct-play is fully seekable
        return;
      }
      const rel = target - anchor; // position within the anchored stream
      // Native ONLY if the target is actually BUFFERED (downloaded) - `seekable`
      // over-reports the full duration before it is produced, which would seek
      // into a hole. Otherwise re-anchor: a fresh session remuxed at `target`.
      let buffered = false;
      for (let i = 0; i < v.buffered.length; i += 1) {
        if (rel >= v.buffered.start(i) - 0.5 && rel <= v.buffered.end(i) + 0.5) {
          buffered = true;
          break;
        }
      }
      if (buffered) {
        v.currentTime = Math.max(0, rel);
      } else {
        setAnchor(target);
      }
    },
    [decision.kind, anchor, knownDurationMs],
  );

  const getPosition = useCallback(() => baseSec + (videoRef.current?.currentTime ?? 0), [baseSec]);

  const skip = useCallback(
    (delta: number) => {
      // `seekTo` is absolute, and the element clock is relative to the anchor, so
      // skip from the ABSOLUTE position (getPosition), not raw currentTime.
      if (!videoRef.current) return;
      seekTo(getPosition() + delta);
    },
    [seekTo, getPosition],
  );

  const clientXToSec = useCallback(
    (clientX: number): number | null => {
      const v = videoRef.current;
      const bar = barRef.current;
      let total: number;
      if (knownDurationMs) total = knownDurationMs / 1000;
      else if (Number.isFinite(v?.duration)) total = (v as HTMLVideoElement).duration;
      else total = 0;
      if (!v || !bar || !total) return null;
      const rect = bar.getBoundingClientRect();
      const pct = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      return pct * total;
    },
    [knownDurationMs],
  );

  const scrubToClientX = useCallback(
    (clientX: number) => {
      const s = clientXToSec(clientX);
      if (s != null) setScrubPreview(s);
    },
    [clientXToSec],
  );
  const commitScrub = useCallback(() => {
    const s = scrubPreviewRef.current;
    setScrubPreview(null);
    if (s != null) seekTo(s);
  }, [seekTo]);
  const seekToClientX = useCallback(
    (clientX: number) => {
      const s = clientXToSec(clientX);
      if (s != null) seekTo(s);
    },
    [clientXToSec, seekTo],
  );

  const onBarMove = useCallback(
    (e: React.PointerEvent) => {
      const bar = barRef.current;
      if (!bar || !dur) return;
      const rect = bar.getBoundingClientRect();
      const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
      setHover({ x: pct * rect.width, t: pct * dur, w: rect.width });
      if (scrubbing) setScrubPreview(pct * dur);
    },
    [dur, scrubbing],
  );

  const setVol = useCallback((val: number) => {
    const v = videoRef.current;
    if (!v) return;
    v.volume = Math.max(0, Math.min(1, val));
    v.muted = v.volume === 0;
  }, []);

  const toggleMute = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    v.muted = !v.muted;
  }, []);

  const applyRate = useCallback((r: number) => {
    const v = videoRef.current;
    if (v) v.playbackRate = r;
  }, []);

  const toggleFullscreen = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    if (document.fullscreenElement) {
      void document.exitFullscreen();
      return;
    }
    if (document.fullscreenEnabled && typeof el.requestFullscreen === 'function') {
      void el.requestFullscreen();
      return;
    }
    // iPhone Safari has no element fullscreen API → the video's native one.
    const v = videoRef.current as
      | (HTMLVideoElement & { webkitEnterFullscreen?: () => void })
      | null;
    if (typeof v?.webkitEnterFullscreen === 'function') v.webkitEnterFullscreen();
  }, []);

  // Switch audio language. For HLS, RE-ANCHOR at the current position rather than
  // hls.js's in-place `audioTrack` swap: the in-place swap can leave the new audio
  // out of sync with the picture, whereas a re-anchor is a clean fresh attach that
  // loads the chosen rendition correctly (a brief reload, like a seek). Direct-play
  // has a single audio track, so nothing to switch.
  const setAudio = useCallback(
    (index: number) => {
      if (index === audioIndexRef.current) return;
      setAudioIndex(index);
      if (decision.kind !== 'direct') {
        const pos = baseSec + (videoRef.current?.currentTime ?? 0);
        setAnchor(Math.max(0, Math.floor(pos)));
      }
    },
    [decision.kind, baseSec],
  );

  // Switch the playback engine (Settings). Persist + re-anchor at the current
  // absolute position so the new pipeline attaches there (a brief reload, like a
  // seek); clear any stale direct-play fallback.
  const setEnginePref = useCallback(
    (p: WebEnginePref) => {
      setWebEnginePref(p);
      setForceHls(false);
      setEnginePrefState(p);
      setAnchor(Math.max(0, Math.floor(baseSec + (videoRef.current?.currentTime ?? 0))));
    },
    [baseSec],
  );

  return {
    videoRef,
    containerRef,
    barRef,
    enginePref,
    setEnginePref,
    playing,
    waiting,
    ready,
    cur,
    dur,
    bufEnd,
    volume,
    muted,
    rate,
    fs,
    useHls,
    audioTracks,
    audioIndex,
    setAudio,
    anchor,
    baseSec,
    aac: decision.kind === 'direct' ? false : Boolean(decision.aacMaster),
    hlsRef,
    shakaRef,
    scrubbing,
    setScrubbing,
    scrubPreview,
    scrubToClientX,
    commitScrub,
    hover,
    setHover,
    togglePlay,
    skip,
    seekTo,
    getPosition,
    setVol,
    toggleMute,
    applyRate,
    toggleFullscreen,
    seekToClientX,
    onBarMove,
  };
}
