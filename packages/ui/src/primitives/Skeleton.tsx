// <Skeleton>: the pulsing loading placeholder.
//
// The pulse is an Animated loop rather than a CSS keyframe so the one component
// works on every target. It animates opacity only, which both renderers can keep
// on the compositor, and which a TV GPU handles without a repaint.

import { useEffect, useRef } from 'react';
import { Animated, Easing, type StyleProp, type ViewStyle } from 'react-native';
import { type BoxStyleProps, boxStyle } from '../system/boxStyle';
import { motion, radius } from '../tokens';

export interface SkeletonProps extends BoxStyleProps {
  style?: StyleProp<ViewStyle>;
}

/** The wash the placeholder pulses between. Matches the pre-uikit `bg-white/6`. */
const WASH = 'rgba(255, 255, 255, 0.06)';

export function Skeleton({ style, ...box }: Readonly<SkeletonProps>) {
  const pulse = useRef(new Animated.Value(0.55)).current;

  useEffect(() => {
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulse, {
          toValue: 1,
          duration: motion.duration.slow * 2,
          easing: Easing.inOut(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(pulse, {
          toValue: 0.55,
          duration: motion.duration.slow * 2,
          easing: Easing.inOut(Easing.ease),
          useNativeDriver: true,
        }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [pulse]);

  return (
    <Animated.View
      style={[
        { backgroundColor: WASH, borderRadius: radius.sm },
        boxStyle(box),
        style,
        { opacity: pulse },
      ]}
    />
  );
}
