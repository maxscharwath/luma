// Geometry for <ProgressRing>, shared by both renderers so the arc is identical
// on every target. Pure maths, so it is unit-tested rather than eyeballed.

import { clamp01 } from './Progress';

export interface RingProps {
  /** Fill fraction, 0..1 (clamped). */
  value: number;
  /** Outer diameter. */
  size?: number;
  /** Stroke width. */
  stroke?: number;
  /** Unfilled track colour. */
  track?: string;
  /** Filled (progress) colour. */
  fill?: string;
}

export interface RingGeometry {
  size: number;
  stroke: number;
  track: string;
  fill: string;
  centre: number;
  radius: number;
  circumference: number;
  /** How much of the circumference to hide, i.e. the unfilled remainder. */
  dashOffset: number;
}

export function ringGeometry({
  value,
  size = 22,
  stroke = 2.5,
  track = 'rgba(255, 255, 255, 0.12)',
  fill = 'rgba(255, 255, 255, 0.6)',
}: Readonly<RingProps>): RingGeometry {
  // The stroke straddles the path, so the radius is inset by half of it or the
  // ring would be clipped by the viewBox.
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  return {
    size,
    stroke,
    track,
    fill,
    centre: size / 2,
    radius,
    circumference,
    dashOffset: circumference * (1 - clamp01(value)),
  };
}

/** SVG draws an arc from 3 o'clock; the design starts it at 12. */
export const RING_ROTATION = '-90deg';
