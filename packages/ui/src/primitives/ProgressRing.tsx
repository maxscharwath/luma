// <ProgressRing> on the native targets: react-native-svg, same geometry as the
// web renderer. There is no CSS transition here, so the arc steps with each
// value change; poll results arrive slowly enough that it reads as progress
// rather than as a jump.

import { View } from 'react-native';
import Svg, { Circle } from 'react-native-svg';
import { RING_ROTATION, type RingProps, ringGeometry } from './ring';

export type { RingProps as ProgressRingProps } from './ring';

export function ProgressRing(props: Readonly<RingProps>) {
  const g = ringGeometry(props);
  return (
    <View style={{ transform: [{ rotate: RING_ROTATION }] }}>
      <Svg width={g.size} height={g.size} viewBox={`0 0 ${g.size} ${g.size}`}>
        <Circle
          cx={g.centre}
          cy={g.centre}
          r={g.radius}
          fill="none"
          stroke={g.track}
          strokeWidth={g.stroke}
        />
        <Circle
          cx={g.centre}
          cy={g.centre}
          r={g.radius}
          fill="none"
          stroke={g.fill}
          strokeWidth={g.stroke}
          strokeLinecap="round"
          strokeDasharray={g.circumference}
          strokeDashoffset={g.dashOffset}
        />
      </Svg>
    </View>
  );
}
