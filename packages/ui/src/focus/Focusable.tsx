// The one focusable primitive. Every remote-reachable control on every platform
// is this component.
//
// It is a plain `Pressable`, which is genuinely universal: on Apple TV and
// Android TV the OS focus engine drives it and Select fires `onPress`; under
// react-native-web it renders a `tabIndex=0` element whose focus state is
// tracked and whose Enter key fires `onPress`. The only platform-specific bits
// are the host props (`hasTVPreferredFocus` vs `data-focus`) and how the focus
// transition is animated, both behind `./nav` and `./transition`.
//
// Ring and scale are applied to the SAME element, because a box-shadow scales
// with its element's transform: ring the Pressable but scale a child and the
// amber outline would visibly detach from the artwork it is meant to outline.

import { type ReactNode, useCallback, useState } from 'react';
import { Animated, Pressable, type StyleProp, type ViewStyle } from 'react-native';
import { ring } from '../tokens';
import { pressGuardActive } from './guard';
import { useFocusHostProps } from './nav';
import { useFocusScale } from './transition';

const AnimatedPressable = Animated.createAnimatedComponent(Pressable);

export interface FocusState {
  focused: boolean;
  pressed: boolean;
}

export interface FocusableProps {
  onPress?: () => void;
  onFocus?: () => void;
  onBlur?: () => void;
  /** Declare this the screen's entry point (tvOS `hasTVPreferredFocus`). */
  autoFocus?: boolean;
  disabled?: boolean;
  /** Scale while focused. The design uses 1.06 for rail tiles, 1.05 for posters
   *  and 1.04 for the primary action; 1 (the default) means "ring only". */
  focusScale?: number;
  /** Draw the signature amber ring while focused. Turn off for controls that
   *  ring an inner element instead (a cast face rings its avatar, not the card). */
  ring?: boolean;
  style?: StyleProp<ViewStyle>;
  /** Merged on top of `style` while focused. */
  focusedStyle?: StyleProp<ViewStyle>;
  children?: ReactNode | ((state: FocusState) => ReactNode);
  /** Accessibility label; also the tvOS VoiceOver name. */
  label?: string;
}

export function Focusable({
  onPress,
  onFocus,
  onBlur,
  autoFocus,
  disabled = false,
  focusScale = 1,
  ring: showRing = true,
  style,
  focusedStyle,
  children,
  label,
}: Readonly<FocusableProps>) {
  const [focused, setFocused] = useState(false);
  const [pressed, setPressed] = useState(false);
  const hostProps = useFocusHostProps({ autoFocus, disabled });
  const animated = useFocusScale(focused, focusScale);

  const handleFocus = useCallback(() => {
    setFocused(true);
    onFocus?.();
  }, [onFocus]);

  const handleBlur = useCallback(() => {
    setFocused(false);
    onBlur?.();
  }, [onBlur]);

  // The OK guard lives here rather than in the navigation engine: both platforms
  // fire `onPress` from their own key handling (RNW's Enter, tvOS's Select), so
  // this is the single choke point that can swallow the tail of the press that
  // opened the screen.
  const press = useCallback(() => {
    if (disabled || pressGuardActive()) return;
    onPress?.();
  }, [disabled, onPress]);

  return (
    <AnimatedPressable
      {...hostProps}
      disabled={disabled}
      onPress={press}
      onPressIn={() => setPressed(true)}
      onPressOut={() => setPressed(false)}
      onFocus={handleFocus}
      onBlur={handleBlur}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={[
        style,
        focused ? focusedStyle : null,
        showRing && focused ? { boxShadow: ring.focusLift } : null,
        animated,
      ]}
    >
      {typeof children === 'function' ? children({ focused, pressed }) : children}
    </AnimatedPressable>
  );
}
