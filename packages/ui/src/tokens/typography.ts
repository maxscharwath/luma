// Type roles. THE SOURCE OF TRUTH for both renderers (see gen-token-css.ts).
//
// The design authors line height as a RATIO and tracking in `em`, which is what
// the CSS `font` shorthand wants. React Native has neither: it needs an absolute
// `lineHeight` and an absolute `letterSpacing` in px. So each role stores the
// authored ratio/em and derives the pixel values, rather than storing pixels and
// dividing back (which round-trips lossily: 1.55 would come back as 1.56).
//
// The families are loaded per platform but named identically everywhere: a
// <link> in the TV shells' index.html on web, expo-font on Apple TV / Android TV.

import type { TextStyle } from 'react-native';

export const fonts = {
  display: 'Bricolage Grotesque',
  ui: 'Hanken Grotesk',
} as const;

/** Tracking as authored, in em. */
export const tracking = { overline: 0.12, display: -0.02 } as const;

/** A role exactly as the design states it. `family` picks the CSS var too. */
export interface TypeSpec {
  family: keyof typeof fonts;
  weight: '400' | '500' | '600' | '700';
  size: number;
  /** Unitless line-height ratio (the CSS shorthand's `/ n`). */
  ratio: number;
  /** Letter spacing in em; omitted means none. */
  em?: number;
  uppercase?: boolean;
}

export const typeSpec = {
  hero: { family: 'display', weight: '700', size: 66, ratio: 0.98, em: tracking.display },
  h1: { family: 'display', weight: '700', size: 38, ratio: 1, em: tracking.display },
  h2: { family: 'display', weight: '700', size: 22, ratio: 1.1, em: tracking.display },
  title: { family: 'display', weight: '700', size: 20, ratio: 1.05, em: tracking.display },
  body: { family: 'ui', weight: '400', size: 16, ratio: 1.55 },
  label: { family: 'ui', weight: '600', size: 15, ratio: 1.3 },
  meta: { family: 'ui', weight: '500', size: 13, ratio: 1.4 },
  overline: {
    family: 'ui',
    weight: '700',
    size: 11,
    ratio: 1,
    em: tracking.overline,
    uppercase: true,
  },
} as const satisfies Record<string, TypeSpec>;

export type TypeRole = keyof typeof typeSpec;

/** Round to 2 decimals: React Native accepts fractional px, but a stable value
 * keeps snapshot diffs and the generated CSS quiet. */
const px = (n: number) => Math.round(n * 100) / 100;

function toStyle(s: TypeSpec): TextStyle {
  return {
    fontFamily: fonts[s.family],
    fontWeight: s.weight,
    fontSize: s.size,
    lineHeight: Math.round(s.size * s.ratio),
    ...(s.em === undefined ? null : { letterSpacing: px(s.size * s.em) }),
    ...(s.uppercase ? { textTransform: 'uppercase' as const } : null),
  };
}

/** The roles as ready-to-spread React Native text styles. */
export const type = Object.fromEntries(
  Object.entries(typeSpec).map(([k, v]) => [k, toStyle(v)]),
) as Record<TypeRole, TextStyle>;
