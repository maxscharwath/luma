// Live transport metrics from whichever MSE engine is actually playing the HLS
// master (Shaka Player or hls.js), plus a measured-FPS sampler. Kept apart from
// `web-stats.ts` so the stats builder stays engine-agnostic and both files stay
// small. Direct-play / native HLS expose no engine handle, so they get nothing
// here (the panel simply omits the rows).

import type { HlsInstance, ShakaPlayerLike } from '#web/features/playback/video-engine';

/** Engine-reported live metrics, normalized to friendly units. Every field is
 * optional: hls.js exposes fewer than Shaka, and both are absent on direct-play. */
export interface EngineLiveStats {
  /** Bitrate of the currently-playing variant (video+audio), kbps. */
  streamBitrateKbps?: number;
  /** Rolling bandwidth estimate the engine measured, kbps. */
  estBandwidthKbps?: number;
  /** Rebuffering events detected this session. */
  stalls?: number;
  /** Total time spent buffering/stalled this session, seconds. */
  bufferingSec?: number;
  /** Bytes fetched over the network this session. */
  bytesDownloaded?: number;
  /** Active codec string the engine is decoding, e.g. "avc1.640028,mp4a.40.2". */
  currentCodecs?: string;
}

/**
 * Read live metrics from the active engine. Shaka's `getStats()` is the richest
 * source (bandwidth estimate, stalls, buffering time, bytes, codecs); hls.js
 * offers the ABR bandwidth estimate and the current level's declared bitrate.
 * Returns null when neither engine is attached (direct-play / native HLS).
 */
export function readEngineStats(
  hls: HlsInstance | null,
  shaka: ShakaPlayerLike | null,
): EngineLiveStats | null {
  if (shaka) {
    try {
      const s = shaka.getStats();
      return {
        streamBitrateKbps: s.streamBandwidth ? Math.round(s.streamBandwidth / 1000) : undefined,
        estBandwidthKbps: s.estimatedBandwidth
          ? Math.round(s.estimatedBandwidth / 1000)
          : undefined,
        stalls: Number.isFinite(s.stallsDetected) ? s.stallsDetected : undefined,
        bufferingSec: Number.isFinite(s.bufferingTime) ? s.bufferingTime : undefined,
        bytesDownloaded: s.bytesDownloaded || undefined,
        currentCodecs: s.currentCodecs || undefined,
      };
    } catch {
      return null; // Shaka throws from getStats() before the first load resolves
    }
  }
  if (hls) {
    const est = hls.bandwidthEstimate;
    const level = hls.levels?.[hls.currentLevel];
    return {
      streamBitrateKbps: level?.bitrate ? Math.round(level.bitrate / 1000) : undefined,
      estBandwidthKbps: Number.isFinite(est) && est > 0 ? Math.round(est / 1000) : undefined,
    };
  }
  return null;
}

/**
 * A stateful frame-rate sampler. Call it each poll tick with the `<video>`; it
 * returns the measured playback FPS (decoded frames over wall-clock elapsed) once
 * two samples exist, else undefined. Lightly smoothed so it doesn't jitter
 * between polls, and it holds the last value while paused (no frames advancing).
 * `now` is injectable for deterministic tests.
 */
export function makeFpsSampler(now: () => number = () => performance.now()) {
  let lastFrames = 0;
  let lastT = 0;
  let fps: number | undefined;
  return (v: HTMLVideoElement | null): number | undefined => {
    const q = v?.getVideoPlaybackQuality?.();
    if (!q) return fps;
    const t = now();
    const frames = q.totalVideoFrames;
    if (!lastT) {
      lastFrames = frames;
      lastT = t;
      return fps;
    }
    const dt = (t - lastT) / 1000;
    if (dt >= 0.25) {
      const df = frames - lastFrames;
      if (df > 0) {
        const inst = df / dt;
        fps = fps ? fps * 0.6 + inst * 0.4 : inst;
      }
      lastFrames = frames;
      lastT = t;
    }
    return fps && fps > 0 ? fps : undefined;
  };
}
