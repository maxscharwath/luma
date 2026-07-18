// Unified player (§14): ONE chrome for web + TV, styled with Tailwind (both
// clients @source this dir; legacy-safe flex-only + no /opacity, see ./tw).
// Barrel for the public surface consumed by the web + TV wrappers.

export { useAudioFilter } from './audio-filter';
export type { CreditsCardItem } from './CreditsCard';
export { currentChapter, currentChapterIndex, normalizeChapters } from './chapters';
export { clamp01, endsAtClock, pct } from './fmt';
export type { PlayerProps } from './Player';
export { Player } from './Player';
export type { SubtitleGenBundle, SubtitleGenRequest } from './settings/gen';
export {
  DEFAULT_SUB_APPEARANCE,
  SUB_COLORS,
  type SubEdge,
  type SubFont,
  type SubSize,
  type SubtitleAppearance,
  subtitleCss,
  useSubtitleAppearance,
} from './subtitle-appearance';
export type {
  AudioFilterMode,
  Chapter,
  PlaneRect,
  PlayerController,
  PlayerEngineOption,
  PlayerFlags,
  PlayerQuality,
  PlayerStats,
  PlayerSub,
  PlayerSurface,
} from './types';
export { TV_FLAGS, WEB_FLAGS } from './types';
export type { UpNextData, UpNextItem } from './UpNextSheet';
