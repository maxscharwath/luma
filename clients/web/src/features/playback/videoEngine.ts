// The imperative playback engine for the player: the `<video>` element event
// wiring and the source decision (direct-play vs an HLS audio-transcode / a
// seamless multi-track master, including hls.js attach + rendition selection).
// `useVideoPlayback` owns the React state/effects and drives these helpers.

import type { AudioTrack } from '@luma/core';
import { audioTracksOf, masterNeedsAac, planAudio, restorePlaybackAfterSwap } from '@luma/core';
import { lumaClient, type MovieView } from '#web/shared/lib/api';

type HlsInstance = import('hls.js').default;

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

/** State setters the media-event listeners feed into. */
export interface MediaEventSetters {
  setCur: (n: number) => void;
  setDur: (n: number) => void;
  setBufEnd: (n: number) => void;
  setPlaying: (b: boolean) => void;
  setWaiting: (b: boolean) => void;
  setVolume: (n: number) => void;
  setMuted: (b: boolean) => void;
  setRate: (n: number) => void;
}

/**
 * Subscribe the media element's events to the hook's state setters (time /
 * duration / buffer / play-pause / waiting / volume / rate). Returns the
 * unsubscribe cleanup.
 */
export function bindMediaEvents(
  v: HTMLVideoElement,
  item: MovieView,
  baseSecRef: { readonly current: number },
  setters: MediaEventSetters,
): () => void {
  const { setCur, setDur, setBufEnd, setPlaying, setWaiting, setVolume, setMuted, setRate } =
    setters;
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
}

/** Inputs for {@link attachMediaSource}. */
export interface AttachSourceOptions {
  v: HTMLVideoElement;
  item: MovieView;
  seamless: boolean;
  audioIndex: number;
  baseSecRef: { readonly current: number };
  audioIndexRef: { readonly current: number };
  hlsRef: { current: HlsInstance | null };
  setUseHls: (b: boolean) => void;
}

/**
 * Point the media element at the right source. Seamless mode attaches the HLS
 * master ONCE (audio switches happen in place see {@link applySeamlessRendition}
 * so the source is NOT re-pointed and the picture never moves). Otherwise the
 * source re-attaches per chosen track (direct-play, or a per-track AAC remux for
 * audio this runtime can't decode → reloads on switch). Returns the teardown
 * cleanup and flips `setUseHls`.
 */
export function attachMediaSource(opts: AttachSourceOptions): () => void {
  const { v, item, seamless, audioIndex, baseSecRef, audioIndexRef, hlsRef, setUseHls } = opts;

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
  // SSR-serializable object a function on it breaks Seroval dehydration.
  const url = lumaClient().hlsAudioUrl(item.id, plan.index, plan.copy);
  let destroyed = false;
  let hls: HlsInstance | null = null;

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
}

/** Inputs for {@link applySeamlessRendition}. */
export interface SeamlessRenditionOptions {
  item: MovieView;
  audioIndex: number;
  hlsRef: { current: HlsInstance | null };
  videoEl: HTMLVideoElement | null;
}

/**
 * Seamless language switch: select the matching audio rendition IN PLACE no
 * source reload, so the video keeps playing at the same position while the audio
 * rendition swaps.
 */
export function applySeamlessRendition(opts: SeamlessRenditionOptions): void {
  const { item, audioIndex, hlsRef, videoEl } = opts;
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
      /* manifest not parsed yet the default rendition is already correct */
    }
    return;
  }
  // Native HLS (Safari/iOS): toggle the matching audioTracks entry.
  type NativeAudioTracks = { length: number; [i: number]: { enabled: boolean } };
  const tracks = (videoEl as unknown as { audioTracks?: NativeAudioTracks } | null)?.audioTracks;
  if (tracks?.length) {
    for (let i = 0; i < tracks.length; i += 1) tracks[i]!.enabled = i === rendition;
  }
}
