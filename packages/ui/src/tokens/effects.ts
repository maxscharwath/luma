// Elevation, focus rings and motion.
//
// Shadows are `boxShadow` strings rather than the legacy iOS shadow* props:
// React Native 0.76+ implements boxShadow on both native platforms, and
// react-native-web maps it straight to CSS. One value, three renderers.

import { colors } from './colors';

export const shadow = {
  card: '0 10px 28px rgba(0, 0, 0, 0.45)',
  pop: '0 20px 50px rgba(0, 0, 0, 0.55)',
  hero: '0 20px 50px rgba(0, 0, 0, 0.6)',
} as const;

/** The 10-foot focus treatment: a CLEAN solid amber ring plus a dark drop
 * shadow for lift, never an amber bloom, so the ring reads as a crisp border. */
export const ring = {
  focus: `0 0 0 4px ${colors.accent}`,
  focusSm: `0 0 0 3px ${colors.accent}`,
  /** Ring + lift, the combination every focusable actually renders. */
  focusLift: `0 0 0 4px ${colors.accent}, 0 10px 28px rgba(0, 0, 0, 0.5)`,
} as const;

export const glow = {
  accent: '0 6px 22px rgba(242, 180, 66, 0.4)',
  play: '0 6px 22px rgba(242, 180, 66, 0.32)',
} as const;

/** Motion. Durations in milliseconds (React Native's Animated unit); the CSS
 * generator emits seconds. Easing curves are bezier control points, consumable
 * by both `Easing.bezier(...)` and `cubic-bezier(...)`. */
export const motion = {
  bezier: {
    out: [0.22, 1, 0.36, 1],
    spring: [0.34, 1.56, 0.64, 1],
  },
  duration: { fast: 150, base: 200, slow: 400 },
  /** Buttons shrink on press; cards lift on focus. */
  pressScale: 0.95,
  focusLift: -6,
} as const;
