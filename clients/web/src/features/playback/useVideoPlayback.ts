import { audioTracksOf, canSeamlessAudioSwitch } from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  applySeamlessRendition,
  attachMediaSource,
  bindMediaEvents,
  type VideoPlayback,
} from '#web/features/playback/videoEngine';
import type { MovieView } from '#web/shared/lib/api';

// The media-element / hls / track-wiring engine lives in `./videoEngine`; the
// `VideoPlayback` shape is re-exported so call sites keep importing it here.
export type { VideoPlayback } from '#web/features/playback/videoEngine';

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
  // While dragging the scrub bar, the previewed absolute position (s) the thumb
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
    return bindMediaEvents(v, item, baseSecRef, {
      setCur,
      setDur,
      setBufEnd,
      setPlaying,
      setWaiting,
      setVolume,
      setMuted,
      setRate,
    });
  }, []);

  // ----- source wiring --------------------------------------------------------
  // The deps gate `audioIndex` out in seamless mode so switching never re-attaches.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    return attachMediaSource({
      v,
      item,
      seamless,
      audioIndex,
      baseSecRef,
      audioIndexRef,
      hlsRef,
      setUseHls,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item, seamless, baseSec, seamless ? 0 : audioIndex]);

  // ----- seamless language switch: select the rendition IN PLACE --------------
  // Runs on audioIndex change in seamless mode only no source reload, so the
  // video keeps playing at the same position while the audio rendition swaps.
  useEffect(() => {
    if (!seamless) return;
    applySeamlessRendition({ item, audioIndex, hlsRef, videoEl: videoRef.current });
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

  // Seek to an ABSOLUTE position (seconds). Direct-play is a normal range seek.
  // Seamless mode: the current stream is an HLS event playlist remuxed at
  // `-ss = baseSec`, so absolute T maps to element time `T - baseSec`. When the
  // target sits inside what this stream already covers, seek IN PLACE (instant,
  // no ffmpeg respawn); only re-`-ss` the master when the target is BEFORE the
  // stream's base or beyond the produced range (so it's instantly available).
  const seekTo = useCallback(
    (absSec: number) => {
      const v = videoRef.current;
      if (!v) return;
      let total: number;
      if (item.durationMs) total = item.durationMs / 1000;
      else if (Number.isFinite(v.duration)) total = v.duration;
      else total = 0;
      const target = Math.max(0, total ? Math.min(total - 1, absSec) : absSec);
      if (!seamless) {
        v.currentTime = target; // direct-play timeline is absolute
        return;
      }
      const base = baseSecRef.current;
      const rel = target - base;
      const seekableEnd = v.seekable.length ? v.seekable.end(v.seekable.length - 1) : 0;
      // In-place when reachable in the current stream. `rel === 0` (seek to the
      // stream's start, incl. seek-to-0 when base is already 0) lands here too, so
      // the cursor and picture stay aligned instead of the cursor jumping alone.
      if (rel >= 0 && rel <= seekableEnd + 0.5) {
        v.currentTime = rel;
        setCur(target);
        return;
      }
      setBaseSec(target); // reloads the master at -ss=target; currentTime → 0; cur → target
      setCur(target);
    },
    [item, seamless],
  );

  // Absolute current position (s), offset-aware stable getter for progress save.
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
