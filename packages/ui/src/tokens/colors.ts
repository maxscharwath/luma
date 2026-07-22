// KROMA colour tokens. THIS FILE IS THE SINGLE SOURCE OF TRUTH: the CSS custom
// properties in packages/ui/src/styles/tokens/colors.css are GENERATED from it
// (`bun run --filter '@kroma/tv-kit' gen:css`). Never edit the .css by hand.
//
// Values are plain strings so they drop straight into a React Native StyleSheet
// and into CSS alike. No color-mix()/oklch(): those cannot be expressed in RN,
// and the old webOS tier could not parse them either.

export const colors = {
  /* Surfaces: deep cinematic charcoal */
  bg: '#0A0A0C',
  surface1: '#121216',
  surface2: '#1C1C22',
  surface3: '#26262E',
  overlay: 'rgba(18, 18, 22, 0.86)',
  border: 'rgba(255, 255, 255, 0.08)',
  borderStrong: 'rgba(255, 255, 255, 0.14)',

  /* Text on dark */
  text: '#F4F3F0',
  textMuted: 'rgba(244, 243, 240, 0.62)',
  textDim: 'rgba(244, 243, 240, 0.45)',

  /* Brand accent: warm amber */
  accent: '#F4B642',
  accentHover: '#FFC862',
  accentBright: '#FFD262',
  accentInk: '#0A0A0C',
  accentSoft: 'rgba(242, 180, 66, 0.16)',

  /* Semantic + quality badges */
  success: '#46D08D',
  info: '#86A8FF',
  hdr: '#C792EA',
  h265: '#5FD3C4',
  danger: '#E53935',
} as const;

export type ColorToken = keyof typeof colors;

/** The chromatic wheel of the KROMA mark, in segment order. */
export const WHEEL_COLORS = [
  '#F2685C',
  '#F4B642',
  '#5FBF8F',
  '#4F9DE0',
  '#6366F1',
  '#A855F7',
] as const;

/** Billboard / poster shade stops (transparent to page background). Used by the
 * hero gradients, which are a LinearGradient on native and on web alike. */
export const SHADE = {
  transparent: 'rgba(10, 10, 12, 0)',
  mid: 'rgba(10, 10, 12, 0.55)',
  full: '#0A0A0C',
} as const;
