// <Progress>: the determinate bar (resume position on a tile, download, import).
// <ProgressRing>: its circular counterpart, used where a bar would not fit.

import { Box } from '../system/Box';
import { colors, radius } from '../tokens';

export interface ProgressProps {
  /** 0..1. Values outside the range are clamped, so a caller can pass a raw
   *  ratio without guarding against a stale duration of 0. */
  value: number;
  /** Bar thickness. The design uses 6 on a rail tile. */
  size?: number;
  color?: string;
  trackColor?: string;
  /** Round the ends. Off for the flush bar pinned to a tile's bottom edge. */
  rounded?: boolean;
}

export function clamp01(n: number): number {
  if (!Number.isFinite(n) || n < 0) return 0;
  return n > 1 ? 1 : n;
}

export function Progress({
  value,
  size = 6,
  color = colors.accent,
  trackColor = 'rgba(255, 255, 255, 0.25)',
  rounded = false,
}: Readonly<ProgressProps>) {
  const pct = `${clamp01(value) * 100}%` as const;
  return (
    <Box
      h={size}
      self="stretch"
      bg={trackColor}
      radius={rounded ? radius.pill : 0}
      overflow="hidden"
      accessibilityRole="progressbar"
    >
      <Box h={size} w={pct} bg={color} radius={rounded ? radius.pill : 0} />
    </Box>
  );
}
