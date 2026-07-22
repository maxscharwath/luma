// <TextField>: the bordered entry field, in both of the modes a 10-foot app
// needs.
//
// On a shell with a physical keyboard (the desktop app, a dev browser) it is a
// real, focusable TextInput the user types into. On an actual TV, where typing
// goes through the on-screen keyboard, it is a NON-focusable display of the
// current value plus a blinking caret, so nothing invites a click that would
// summon the platform IME. The two modes are pixel-matched by construction:
// they share the field, the icon slot and the text style.
//
// The field itself owns the focus visual (a calm accent border) rather than
// letting the 10-foot amber ring box the inner control, which is the shadcn
// InputGroup behaviour this replaces.

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Animated, type StyleProp, TextInput, type TextStyle } from 'react-native';
import { Box, type BoxProps } from '../system/Box';
import { colors, radius as radii } from '../tokens';
import { Icon, type IconName } from './Icon';
import { Txt } from './Text';

export interface TextFieldProps extends Omit<BoxProps, 'children' | 'onChange'> {
  value: string;
  onChange: (next: string) => void;
  /** Fired on Enter. The on-screen keyboard has its own submit key. */
  onSubmit?: () => void;
  placeholder?: string;
  /** Leading glyph inside the field. */
  icon?: IconName;
  /** Control rendered after the entry, inside the field (a Detect button, a
   *  clear button). It keeps its own focus treatment. */
  trailing?: ReactNode;
  /** True when the shell has a real keyboard: renders an editable TextInput.
   *  False (a TV) renders the value plus a blinking caret. */
  physicalKeyboard?: boolean;
  /** Focus on mount so a keyboard user can type immediately. */
  autoFocus?: boolean;
  keyboardType?: 'default' | 'url' | 'email-address';
  label?: string;
  /** Type of the value and the placeholder. */
  textStyle?: StyleProp<TextStyle>;
}

export function TextField({
  value,
  onChange,
  onSubmit,
  placeholder,
  icon,
  trailing,
  physicalKeyboard = false,
  autoFocus = true,
  keyboardType = 'default',
  label,
  textStyle,
  ...box
}: Readonly<TextFieldProps>) {
  const [focused, setFocused] = useState(false);
  return (
    <Box
      row
      align="center"
      gap={14}
      px={22}
      radius="2xl"
      borderWidth={1}
      {...box}
      style={[{ borderColor: focused ? colors.accent : colors.borderStrong }, box.style]}
    >
      {icon ? <Icon name={icon} size={24} stroke={1.8} color="rgba(244, 243, 240, 0.5)" /> : null}
      {physicalKeyboard ? (
        <TextInput
          value={value}
          onChangeText={onChange}
          onSubmitEditing={onSubmit}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          placeholder={placeholder}
          placeholderTextColor={PLACEHOLDER}
          accessibilityLabel={label}
          autoFocus={autoFocus}
          autoCorrect={false}
          autoCapitalize="none"
          spellCheck={false}
          keyboardType={keyboardType}
          selectionColor={colors.accent}
          style={[INPUT, { color: colors.text }, textStyle]}
        />
      ) : (
        <Box row align="center" flex gap={2}>
          <Txt
            lines={1}
            style={[{ flexShrink: 1 }, textStyle]}
            color={value ? 'text' : PLACEHOLDER}
          >
            {value || placeholder || ''}
          </Txt>
          <Caret />
        </Box>
      )}
      {trailing}
    </Box>
  );
}

const PLACEHOLDER = 'rgba(244, 243, 240, 0.3)';

const INPUT = {
  flex: 1,
  minWidth: 0,
  borderWidth: 0,
  backgroundColor: 'transparent',
  padding: 0,
  outlineWidth: 0,
} as const;

/** The blinking insertion point shown in on-screen-keyboard mode. Animated
 * rather than a CSS keyframe so it runs on every target. */
function Caret() {
  const blink = useRef(new Animated.Value(1)).current;
  useEffect(() => {
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(blink, { toValue: 0, duration: 550, useNativeDriver: true }),
        Animated.timing(blink, { toValue: 1, duration: 550, useNativeDriver: true }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [blink]);
  return (
    <Animated.View
      style={{
        width: 2,
        height: 28,
        borderRadius: radii.sm,
        backgroundColor: colors.accent,
        opacity: blink,
      }}
    />
  );
}
