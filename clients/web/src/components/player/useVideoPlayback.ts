import type { AudioTrack } from '@luma/core';
import {
  audioTracksOf,
  canSeamlessAudioSwitch,
  masterNeedsAac,
  planAudio,
  restorePlaybackAfterSwap,
} from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { lumaClient, type MovieView } from '#web/lib/api';

export interface VideoPlayback {
  videoRef: React.RefObject<HTMLVideoElement>;
  containerRef: React.RefObject<HTMLDivElement>;
  barRef: React.RefObject<HTMLDivElement>;
  playing: boolean;
  waiting: boolean;
  cur: number;
  dur: number;
  bufEnd: number;
  volume: number;
  muted: boolean;
  rate: number;
  fs: boolean;
  /** True when audio is delivered via an HLS remux variant (track switch or
   * the AAC fallback) rather than plain direct-play. */
  useHls: boolean;
  /** Every audio track, for the picker. */
  audioTracks: AudioTrack[];
  /** Index of the currently-selected audio track. */
  audioIndex: number;
  /** Switch to the audio track with this audio-relative index. */
  setAudio: (index: number) => void;
  scrubbing: boolean;
  setScrubbing: (v: boolean) => void;
  /** Previewed absolute position (s) while dragging the bar, else null. */
  scrubPreview: number | null;
  /** Preview the scrub position at a client X (no seek yet). */
  scrubToClientX: (clientX: number) => void;
  /** Commit the previewed scrub position (actually seeks). */
  commitScrub: () => void;
  hover: { x: number; t: number } | null;
  setHover: (h: { x: number; t: number } | null) => void;
  togglePlay: () => void;
  skip: (delta: number) => void;
  /** Seek to an absolute position in seconds (offset-aware in seamless mode). */
  seekTo: (absSec: number) => void;
  /** Read the absolute current position in seconds (offset-aware). */
  getPosition: () => number;
  /** Absolute position where the current stream starts (server -ss offset). */
  baseSec: number;
  setVol: (val: number) => void;
  toggleMute: () => void;
  applyRate: (r: number) => void;
  toggleFullscreen: () => void;
  togglePip: () => void;
  seekToClientX: (clientX: number) => void;
  onBarMove: (e: React.PointerEvent) => void;
}

/**
 * Owns the `<video>` element: playback state (time/duration/buffer/volume/rate),
 * the source decision (direct-play `<video src>` vs an HLS audio-transcode for
 * codecs the browser can't decode), fullscreen tracking, and every transport
 * action. Capability detection needs the DOM, so the source is resolved post-mount.
 */
export function useVideoPlayback(item: MovieView): VideoPlayback {
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const barRef = useRef<HTMLDivElement>(null);

  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(false);
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
  const [hover, setHover] = useState<{ x: number; t: number } | null>(null);
  const [scrubbing, setScrubbing] = useState(false);
  // While dragging the scrub bar, the previewed absolute position (s) — the thumb
  // follows it but we only COMMIT the seek on release, so a seamless (-ss) stream
  // isn't reloaded on every mouse-move (which never settled at the drop point).
  const [scrubPreview, setScrubPreview] = useState<number | null>(null);
  const scrubPreviewRef = useRef<number | null>(null);
  scrubPreviewRef.current = scrubPreview;
  // Absolute position (s) where the current seamless stream starts. The server
  // `-ss` remux restarts the timeline at 0, so the REAL position is
  // baseSec + video.currentTime. Always 0 in direct-play (timeline is absolute).
  const [baseSec, setBaseSec] = useState(0);
  const baseSecRef = useRef(0);
  baseSecRef.current = baseSec;
  // Latest selected audio index, for the manifest-parsed re-select after a reload.
  const audioIndexRef = useRef(0);
  audioIndexRef.current = audioIndex;

  const audioTracks = audioTracksOf(item);
  // Seamless multi-language: one HLS master with every track as a rendition →
  // language switches happen in place (no reload, the picture never moves).
  const seamless = useMemo(() => canSeamlessAudioSwitch(item), [item]);
  const hlsRef = useRef<import('hls.js').default | null>(null);

  // ----- video element wiring -------------------------------------------------
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    // Real position = stream offset (server -ss base) + element time.
    const onTime = () => setCur(baseSecRef.current + v.currentTime);
    // Prefer the catalogue runtime: a per-track HLS remux is a growing event
    // playlist whose `video.duration` is Infinity/growing, which desyncs the bar.
    const onDur = () => {
      const total = item.durationMs ? item.durationMs / 1000 : 0;
      if (total > 0) setDur(total);
      else if (Number.isFinite(v.duration)) setDur(v.duration);
    };
    const onProg = () => setBufEnd(v.buffered.length ? v.buffered.end(v.buffered.length - 1) : 0);
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onWaiting = () => setWaiting(true);
    const onPlaying = () => setWaiting(false);
    const onVol = () => {
      setVolume(v.volume);
      setMuted(v.muted);
    };
    const onRate = () => setRate(v.playbackRate);
    v.addEventListener('timeupdate', onTime);
    v.addEventListener('durationchange', onDur);
    v.addEventListener('progress', onProg);
    v.addEventListener('play', onPlay);
    v.addEventListener('pause', onPause);
    v.addEventListener('waiting', onWaiting);
    v.addEventListener('playing', onPlaying);
    v.addEventListener('volumechange', onVol);
    v.addEventListener('ratechange', onRate);
    return () => {
      v.removeEventListener('timeupdate', onTime);
      v.removeEventListener('durationchange', onDur);
      v.removeEventListener('progress', onProg);
      v.removeEventListener('play', onPlay);
      v.removeEventListener('pause', onPause);
      v.removeEventListener('waiting', onWaiting);
      v.removeEventListener('playing', onPlaying);
      v.removeEventListener('volumechange', onVol);
      v.removeEventListener('ratechange', onRate);
    };
  }, []);

  // ----- source wiring --------------------------------------------------------
  // Seamless mode attaches the HLS master ONCE (audio switches happen in place —
  // see the next effect — so the source is NOT re-pointed and the picture never
  // moves). Otherwise the source re-attaches per chosen track (direct-play, or a
  // per-track AAC remux for audio this runtime can't decode → reloads on switch).
  // The deps gate `audioIndex` out in seamless mode so switching never re-attaches.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;

    if (seamless) {
      setUseHls(true);
      // -ss the master at baseSec so resume/seek to any position is available
      // immediately (the stream restarts at 0; baseSec is added back for display).
      const url = lumaClient().hlsMasterUrl(item.id, masterNeedsAac(item), baseSecRef.current);
      let destroyed = false;
      if (v.canPlayType('application/vnd.apple.mpegurl')) {
        v.src = url; // Safari/iOS: native HLS exposes renditions on video.audioTracks
      } else {
        void import('hls.js').then(({ default: Hls }) => {
          if (destroyed) return;
          if (!Hls.isSupported()) {
            v.src = url;
            return;
          }
          const hls = new Hls({ enableWorker: true, lowLatencyMode: false });
          hlsRef.current = hls;
          // A reload (seek/-ss change) recreates hls → it would revert to the
          // master's default audio. Re-select the user's track once the manifest
          // is parsed so the chosen language survives seeks. (Live switches are
          // handled by the in-place effect below.)
          hls.on(Hls.Events.MANIFEST_PARSED, () => {
            const order = audioTracksOf(item);
            const rendition = order.findIndex((t) => t.index === audioIndexRef.current);
            if (rendition > 0) {
              try {
                hls.audioTrack = rendition;
              } catch {
                /* ignore */
              }
            }
          });
          hls.loadSource(url);
          hls.attachMedia(v);
        });
      }
      return () => {
        destroyed = true;
        hlsRef.current?.destroy();
        hlsRef.current = null;
        v.removeAttribute('src');
        v.load();
      };
    }

    // Non-seamless: per-track remux / direct-play. Re-attaches on track change.
    const plan = planAudio(item, audioIndex);
    setUseHls(plan.mode === 'hls');

    // Capture position + play state BEFORE re-pointing the source (assigning the
    // new src resets currentTime to 0).
    const resumeAt = v.currentTime || 0;
    const wasPlaying = !v.paused;

    if (plan.mode === 'direct') {
      v.src = item.stream; // direct-play: server range-streams the original file
      return restorePlaybackAfterSwap(v, resumeAt, wasPlaying);
    }

    // Per-track remux: copy video, copy-or-AAC the chosen audio, delivered as HLS.
    // Built here (not in the loader) so the route's loader data stays a plain,
    // SSR-serializable object — a function on it breaks Seroval dehydration.
    const url = lumaClient().hlsAudioUrl(item.id, plan.index, plan.copy);
    let destroyed = false;
    let hls: import('hls.js').default | null = null;

    if (v.canPlayType('application/vnd.apple.mpegurl')) {
      v.src = url;
    } else {
      void import('hls.js').then(({ default: Hls }) => {
        if (destroyed) return;
        if (!Hls.isSupported()) {
          v.src = url; // last resort
          return;
        }
        hls = new Hls({ enableWorker: true, lowLatencyMode: false });
        hls.loadSource(url);
        hls.attachMedia(v);
      });
    }

    // Restore the old position once the new source makes it seekable.
    const restore = restorePlaybackAfterSwap(v, resumeAt, wasPlaying);
    return () => {
      destroyed = true;
      hls?.destroy();
      restore();
      v.removeAttribute('src');
      v.load();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item, seamless, baseSec, seamless ? 0 : audioIndex]);

  // ----- seamless language switch: select the rendition IN PLACE --------------
  // Runs on audioIndex change in seamless mode only — no source reload, so the
  // video keeps playing at the same position while the audio rendition swaps.
  useEffect(() => {
    if (!seamless) return;
    const order = audioTracksOf(item);
    const rendition = Math.max(
      0,
      order.findIndex((t) => t.index === audioIndex),
    );
    const hls = hlsRef.current;
    if (hls) {
      try {
        hls.audioTrack = rendition; // hls.js renditions are in playlist order
      } catch {
        /* manifest not parsed yet — the default rendition is already correct */
      }
      return;
    }
    // Native HLS (Safari/iOS): toggle the matching audioTracks entry.
    type NativeAudioTracks = { length: number; [i: number]: { enabled: boolean } };
    const tracks = (videoRef.current as unknown as { audioTracks?: NativeAudioTracks } | null)
      ?.audioTracks;
    if (tracks?.length) {
      for (let i = 0; i < tracks.length; i += 1) tracks[i]!.enabled = i === rendition;
    }
    // baseSec in deps: after a seek reloads the master, re-apply the chosen track.
  }, [item, seamless, audioIndex, baseSec]);

  useEffect(() => {
    const onFs = () => setFs(Boolean(document.fullscreenElement));
    document.addEventListener('fullscreenchange', onFs);
    return () => document.removeEventListener('fullscreenchange', onFs);
  }, []);

  // ----- actions --------------------------------------------------------------
  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  }, []);

  // Seek to an ABSOLUTE position (seconds). In seamless mode this re-`-ss`-es the
  // master at that offset (so the target is instantly available, no waiting for a
  // from-0 remux); in direct-play it's a normal range seek.
  const seekTo = useCallback(
    (absSec: number) => {
      const v = videoRef.current;
      if (!v) return;
      let total: number;
      if (item.durationMs) total = item.durationMs / 1000;
      else if (Number.isFinite(v.duration)) total = v.duration;
      else total = 0;
      const target = Math.max(0, total ? Math.min(total - 1, absSec) : absSec);
      if (seamless) {
        setBaseSec(target); // reloads the master at -ss=target; currentTime → 0; cur → target
        setCur(target);
      } else {
        v.currentTime = target; // direct-play timeline is absolute
      }
    },
    [item, seamless],
  );

  // Absolute current position (s), offset-aware — stable getter for progress save.
  const getPosition = useCallback(
    () => baseSecRef.current + (videoRef.current?.currentTime ?? 0),
    [],
  );

  const skip = useCallback(
    (delta: number) => {
      const v = videoRef.current;
      if (!v) return;
      seekTo(baseSecRef.current + v.currentTime + delta);
    },
    [seekTo],
  );

  // Map a client X on the scrub bar → absolute seconds. Catalogue runtime, not
  // v.duration (Infinity for the growing HLS remux → NaN/∞).
  const clientXToSec = useCallback(
    (clientX: number): number | null => {
      const v = videoRef.current;
      const bar = barRef.current;
      let total: number;
      if (item.durationMs) total = item.durationMs / 1000;
      else if (Number.isFinite(v?.duration)) total = (v as HTMLVideoElement).duration;
      else total = 0;
      if (!v || !bar || !total) return null;
      const rect = bar.getBoundingClientRect();
      const pct = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      return pct * total;
    },
    [item],
  );

  // Drag start / move: preview only (thumb follows; no reload). Release: commit.
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
  // Back-compat: a single click on the bar (no drag) jumps immediately.
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
      setHover({ x: pct * rect.width, t: pct * dur });
      if (scrubbing) setScrubPreview(pct * dur); // preview; committed on release
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
    if (document.fullscreenElement) void document.exitFullscreen();
    else void el.requestFullscreen?.();
  }, []);

  const togglePip = useCallback(() => {
    const v = videoRef.current as
      | (HTMLVideoElement & { requestPictureInPicture?: () => Promise<unknown> })
      | null;
    if (!v) return;
    if (document.pictureInPictureElement) void document.exitPictureInPicture();
    else void v.requestPictureInPicture?.();
  }, []);

  // Switching tracks re-runs the source effect, which re-attaches the stream and
  // restores the current position. No-op when the track is already selected.
  const setAudio = useCallback(
    (index: number) => setAudioIndex((cur) => (cur === index ? cur : index)),
    [],
  );

  return {
    videoRef,
    containerRef,
    barRef,
    playing,
    waiting,
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
    baseSec,
    setVol,
    toggleMute,
    applyRate,
    toggleFullscreen,
    togglePip,
    seekToClientX,
    onBarMove,
  };
}
