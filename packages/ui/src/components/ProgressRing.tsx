// A thin circular progress ring (0..1), starting at 12 o'clock and filling
// clockwise. Pure inline SVG + inline styles (no Tailwind opacity modifiers or
// CSS custom properties) so it renders identically on the legacy webOS Chromium
// tier as well as the modern web client.

export interface ProgressRingProps {
  /** Fill fraction, 0..1 (clamped). */
  value: number;
  /** Outer diameter in px. */
  size?: number;
  /** Stroke width in px. */
  stroke?: number;
  /** Unfilled track color. */
  track?: string;
  /** Filled (progress) color. */
  fill?: string;
}

export function ProgressRing({
  value,
  size = 22,
  stroke = 2.5,
  track = 'rgba(255,255,255,0.12)',
  fill = 'rgba(255,255,255,0.6)',
}: Readonly<ProgressRingProps>) {
  const r = (size - stroke) / 2;
  const circ = 2 * Math.PI * r;
  const filled = Math.max(0, Math.min(1, value));
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      style={{ transform: 'rotate(-90deg)' }}
      aria-hidden="true"
    >
      <title>Progress</title>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={track} strokeWidth={stroke} />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke={fill}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={circ}
        strokeDashoffset={circ * (1 - filled)}
        style={{ transition: 'stroke-dashoffset 260ms linear' }}
      />
    </svg>
  );
}
