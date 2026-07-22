// Spacing, radii and the 10-foot design canvas. Numbers (not "16px" strings) so
// they are valid React Native style values; the CSS generator appends the unit.

/** Every TV screen is authored against this fixed canvas and scaled to fit by
 * `<TvStage>`. It is what makes the layout pixel-identical on a 1080p Tizen
 * panel, a 4K Apple TV (1920x1080 points) and an Android TV (960x540 dp). */
export const CANVAS = { width: 1920, height: 1080 } as const;

export const space = {
  1: 4,
  2: 8,
  3: 12,
  4: 16,
  5: 20,
  6: 24,
  8: 32,
  10: 40,
  14: 56,
  16: 64,
} as const;

export const radius = {
  sm: 8,
  md: 10,
  lg: 13,
  xl: 16,
  '2xl': 22,
  pill: 999,
} as const;

/** Layout gutters. `tv` is the 10-foot side padding (overscan-safe on every
 * panel we ship to). */
export const gutter = {
  tv: 64,
  web: 56,
  mobile: 20,
} as const;

/** Component rhythm shared by the rails. */
export const rhythm = {
  rowGap: 18,
  cardWidth: 208,
} as const;

/** Stretch to the positioned parent. React Native 0.86 removed
 * `StyleSheet.absoluteFillObject`, and `StyleSheet.absoluteFill` is an opaque
 * registered style that cannot be spread, so the plain object lives here. */
export const absoluteFill = {
  position: 'absolute',
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
} as const;
