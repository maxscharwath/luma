// <Spinner>: the indeterminate busy ring.
//
// A rotating arc drawn with borders instead of SVG, so it costs one view and no
// per-frame path work: the whole animation is a single native-driven transform,
// which matters on the weakest TV we ship to.

import { useEffect, useRef } from 'react';
import { Animated, Easing } from 'react-native';
import { colors, radius } from '../tokens';

export interface SpinnerProps {
  size?: number;
  /** Ring thickness. Scales with the size by default. */
  thickness?: number;
  color?: string;
}

export function Spinner({
  size = 28,
  thickness = Math.max(2, Math.round(size / 10)),
  color = colors.accent,
}: Readonly<SpinnerProps>) {
  const spin = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const loop = Animated.loop(
      Animated.timing(spin, {
        toValue: 1,
        duration: 900,
        easing: Easing.linear,
        useNativeDriver: true,
      }),
    );
    loop.start();
    return () => loop.stop();
  }, [spin]);

  const rotate = spin.interpolate({ inputRange: [0, 1], outputRange: ['0deg', '360deg'] });

  return (
    <Animated.View
      accessibilityRole="progressbar"
      style={{
        width: size,
        height: size,
        borderRadius: radius.pill,
        borderWidth: thickness,
        // Three transparent quadrants leave a single visible arc, which reads as
        // a spinner the moment it turns.
        borderColor: 'rgba(255, 255, 255, 0.14)',
        borderTopColor: color,
        transform: [{ rotate }],
      }}
    />
  );
}
