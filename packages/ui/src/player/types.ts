import type { AudioTrack } from '@kroma/core';

/**
 * The unified player contract. ONE `<Player>` chrome (packages/ui/src/player)
 * serves both web and TV; each platform provides a `PlayerController` that
 * adapts its engine hook (web `useVideoPlayback`, TV `useDirectPlayback`) plus
 * its app-level state (subtitle selection, generation, resume...) to this shape.
 * The chrome never talks to an engine directly, only through the controller.
 */

/**
 * Per-platform capability flags. A `false` flag removes BOTH the cluster button
 * AND its D-pad focus stop, so there is never a dead button or a trapped focus
 * (see `usePlayerNav`). Everything else is common to every platform.
 */
export interface PlayerFlags {
  /** Volume slider + mute (web). TV delegates volume to the set / amp remote. */
  volume: boolean;
  /** Picture-in-Picture floating window (web). No windowing on TV. */
  pip: boolean;
  /** Fullscreen toggle (web). A TV is already fullscreen. */
  fullscreen: boolean;
}

export const WEB_FLAGS: PlayerFlags = { volume: true, pip: true, fullscreen: true };
export const TV_FLAGS: PlayerFlags = { volume: false, pip: false, fullscreen: false };

/** A subtitle track as the chrome needs it (embedded, downloaded or AI-made). */
export interface PlayerSub {
  index: number;
  language: string | null;
  /** Human label (AI tracks carry their own; else derived from the language). */
  label?: string | null;
  codec: string;
  /** Resolved WebVTT url the renderer fetches (absent for picture subs). */
  url?: string | null;
  /** AI-generated (Whisper / translate) track → violet "IA" treatment. */
  ai?: boolean;
  /** false = a picture sub (PGS/VobSub) we cannot render as text → disabled. */
  selectable: boolean;
  /** Deletable generated-track id (present only for AI tracks). */
  subId?: string | null;
}

/** Volume-normalizer modes (§7). `night` clamps loud peaks hard. */
export type AudioFilterMode = 'off' | 'standard' | 'night';

/** A quality option (§5 Settings). The server is remux-only, so this reflects
 *  the SOURCE (one honest "Auto · <source>" entry) rather than a fake ladder. */
export interface PlayerQuality {
  id: string;
  label: string;
}

/** A playback-engine choice (§5 Settings, web only): Auto / Direct / Remux. Same
 *  shape as a quality option, so the picker component is shared. */
export interface PlayerEngineOption {
  id: string;
  label: string;
}

/** One chapter segment on the progress bar (§1). */
export interface Chapter {
  startMs: number;
  endMs: number;
  title: string;
  /** Marker origin, for the coloured intro/credits treatment when derived. */
  kind?: 'intro' | 'credits' | 'chapter';
}

/** Live "stats for nerds" snapshot (§9). All optional: TV fills what it can. */
export interface PlayerStats {
  resolution?: string;
  videoCodec?: string;
  fps?: string;
  dropped?: string;
  audioFormat?: string;
  bitrate?: string;
  buffer?: string;
  mode?: string;
  extra?: { label: string; value: string }[];
}

/** The video surface kind the controller drives. `video` = an in-page element;
 *  the others render to a native plane BEHIND a transparent page. */
export type PlayerSurface = 'video' | 'avplay' | 'mpv' | 'exo';

/**
 * Everything the shared chrome reads or drives. Values marked "(web)" are no-ops
 * / stable falses on TV (their flag is off, so the chrome never surfaces them).
 */
export interface PlayerController {
  // ---- clock (absolute seconds) ----
  cur: number;
  dur: number;
  bufEnd: number;
  /** Pending scrub target while dragging / D-pad seeking; null when settled. */
  seekPreview: number | null;

  // ---- status ----
  playing: boolean;
  waiting: boolean;
  ready: boolean;
  /** Already-localized warning/error string, or null. */
  error: string | null;
  /** Bumps once each time playback reaches the natural end (autoplay trigger). */
  endedNonce: number;
  surface: PlayerSurface;

  // ---- transport ----
  togglePlay(): void;
  /** Seek to an absolute position (seconds). */
  seekTo(abs: number): void;
  /** Relative skip (seconds); negative = back. */
  skip(delta: number): void;

  // ---- scrub gesture (absolute seconds; shared by pointer + D-pad) ----
  scrubPreview(abs: number | null): void;
  scrubCommit(): void;

  // ---- volume (web) ----
  volume: number;
  muted: boolean;
  setVolume(v: number): void;
  toggleMute(): void;

  // ---- rate + loop ----
  rate: number;
  setRate(r: number): void;
  loop: boolean;
  setLoop(v: boolean): void;

  // ---- audio tracks ----
  audioTracks: AudioTrack[];
  audioIndex: number;
  setAudio(index: number): void;

  // ---- subtitles ----
  subtitles: PlayerSub[];
  subtitleIndex: number | null;
  setSubtitle(index: number | null): void;

  // ---- quality (source-honest) ----
  qualities: PlayerQuality[];
  qualityId: string;
  setQuality(id: string): void;

  // ---- playback engine (web: manual override of the auto direct/HLS decision).
  //      Absent on platforms with no in-player picker (e.g. TV, which offers the
  //      engine in its profile menu), so the Settings row hides itself. ----
  engines?: PlayerEngineOption[];
  engineId?: string;
  setEngine?(id: string): void;

  // ---- audio filter / normalizer ----
  audioFilter: AudioFilterMode;
  setAudioFilter(mode: AudioFilterMode): void;
  audioFilterSupported: boolean;

  // ---- window (web) ----
  pipActive: boolean;
  togglePip(): void;
  fullscreen: boolean;
  toggleFullscreen(): void;

  // ---- stats ----
  getStats(): PlayerStats;
}
