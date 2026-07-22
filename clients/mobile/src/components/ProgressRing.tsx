// Circular progress ring (downloads). Indeterminate (< 0) renders a spinner.

import { ActivityIndicator } from 'react-native';
import Svg, { Circle } from 'react-native-svg';
import { colors } from '../lib/theme';

export function ProgressRing({
  progress,
  size = 34,
  stroke = 2.5,
}: Readonly<{
  /** 0..1, or -1 for indeterminate. */
  progress: number;
  size?: number;
  stroke?: number;
}>) {
  if (progress < 0) return <ActivityIndicator size="small" color={colors.accent} />;
  const r = size / 2 - stroke;
  const c = 2 * Math.PI * r;
  return (
    <Svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <Circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        stroke={colors.borderStrong}
        strokeWidth={stroke}
        fill="none"
      />
      <Circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        stroke={colors.accent}
        strokeWidth={stroke}
        fill="none"
        strokeDasharray={`${c}`}
        strokeDashoffset={c * (1 - Math.max(0.02, progress))}
        strokeLinecap="round"
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
      />
    </Svg>
  );
}
