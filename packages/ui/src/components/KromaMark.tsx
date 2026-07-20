import type { CSSProperties } from 'react';

/** The six wheel segments, clockwise from 12 o'clock: corail, ambre, menthe, azur, indigo, violet. */
export const KROMA_WHEEL_COLORS = [
  '#F2685C',
  '#F4B642',
  '#5FBF8F',
  '#4F9DE0',
  '#6366F1',
  '#A855F7',
] as const;

// Annular sectors (outer r 44, inner r 15, centre 50,50): the hub is a real hole,
// so the mark sits on any surface with no background-matched fill.
export const KROMA_WHEEL_SEGMENTS = [
  'M50 35 L50 6 A44 44 0 0 1 88 28 L62.99 42.5 A15 15 0 0 0 50 35 Z',
  'M62.99 42.5 L88 28 A44 44 0 0 1 88 72 L62.99 57.5 A15 15 0 0 0 62.99 42.5 Z',
  'M62.99 57.5 L88 72 A44 44 0 0 1 50 94 L50 65 A15 15 0 0 0 62.99 57.5 Z',
  'M50 65 L50 94 A44 44 0 0 1 12 72 L37.01 57.5 A15 15 0 0 0 50 65 Z',
  'M37.01 57.5 L12 72 A44 44 0 0 1 12 28 L37.01 42.5 A15 15 0 0 0 37.01 57.5 Z',
  'M37.01 42.5 L12 28 A44 44 0 0 1 50 6 L50 35 A15 15 0 0 0 37.01 42.5 Z',
] as const;

// Spin lives on the <svg> element itself (the wheel is centred in its viewBox),
// not on an inner group: `transform-box: fill-box` is missing on old TV webviews.
const SPIN_CSS = `@keyframes kroma-wheel-spin{to{transform:rotate(360deg)}}
.kroma-wheel-idle{animation:kroma-wheel-spin 9s linear infinite}
.kroma-wheel-loading{animation:kroma-wheel-spin 2.6s linear infinite}
@media (prefers-reduced-motion:reduce){.kroma-wheel-idle,.kroma-wheel-loading{animation:none}}`;

export type KromaMarkSpin = 'idle' | 'loading';

export interface KromaMarkProps {
  /** Width/height of the wheel; a number is px, a string passes through (e.g. ".66em"). */
  size?: number | string;
  /** Continuous rotation: "idle" (9s, ambient) or "loading" (2.6s, spinner). */
  spin?: KromaMarkSpin;
  style?: CSSProperties;
}

/** The KROMA chromatic wheel the standalone brand symbol and the O of the wordmark. */
export function KromaMark({ size = 24, spin, style }: Readonly<KromaMarkProps>) {
  const svg = (
    // viewBox cropped to the wheel bounds so `size` is the true wheel diameter.
    <svg
      width={size}
      height={size}
      viewBox="6 6 88 88"
      aria-hidden="true"
      className={spin ? `kroma-wheel-${spin}` : undefined}
      style={style}
    >
      {KROMA_WHEEL_SEGMENTS.map((d, i) => (
        <path key={d} d={d} fill={KROMA_WHEEL_COLORS[i]} />
      ))}
    </svg>
  );
  if (!spin) return svg;
  return (
    <>
      <style>{SPIN_CSS}</style>
      {svg}
    </>
  );
}
