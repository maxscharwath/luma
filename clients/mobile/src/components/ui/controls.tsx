// Interactive primitives. Press feedback is color-only (~150ms, no transforms),
// matching the product's interaction rules.

import type { ReactNode } from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  type TextInputProps,
  View,
} from 'react-native';
import { colors, radius, spacing } from '../../lib/theme';

export function Button({
  label,
  onPress,
  kind = 'primary',
  disabled,
  loading,
  icon,
}: Readonly<{
  label: string;
  onPress: () => void;
  kind?: 'primary' | 'ghost' | 'danger';
  disabled?: boolean;
  loading?: boolean;
  icon?: ReactNode;
}>) {
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || loading}
      style={({ pressed }) => [
        styles.button,
        kind === 'primary' && { backgroundColor: pressed ? '#E8E6E1' : colors.text },
        kind === 'ghost' && {
          backgroundColor: pressed ? colors.surfaceRaised : colors.surface,
          borderWidth: 1,
          borderColor: colors.border,
        },
        kind === 'danger' && { backgroundColor: pressed ? '#D8574C' : colors.danger },
        (disabled || loading) && { opacity: 0.5 },
      ]}
    >
      {loading ? (
        <ActivityIndicator color={kind === 'primary' ? colors.bg : colors.text} />
      ) : (
        <View style={styles.buttonRow}>
          {icon}
          <Text
            style={[styles.buttonLabel, { color: kind === 'primary' ? colors.bg : colors.text }]}
          >
            {label}
          </Text>
        </View>
      )}
    </Pressable>
  );
}

export function TextField(props: Readonly<TextInputProps>) {
  return (
    <TextInput
      placeholderTextColor={colors.textFaint}
      autoCapitalize="none"
      autoCorrect={false}
      {...props}
      style={[styles.input, props.style]}
    />
  );
}

/** Filled selector chip: quiet surface at rest, accent fill when active. */
export function Chip({
  label,
  active,
  onPress,
}: Readonly<{
  label: string;
  active?: boolean;
  onPress(): void;
}>) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.chip,
        active && { backgroundColor: colors.accent },
        !active && pressed && { backgroundColor: colors.surfaceHigh },
      ]}
    >
      <Text style={[styles.chipLabel, active && { color: colors.accentInk, fontWeight: '800' }]}>
        {label}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  button: {
    minHeight: 48,
    borderRadius: radius.md,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: spacing.lg,
  },
  buttonRow: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  buttonLabel: { fontSize: 15, fontWeight: '700' },
  chip: {
    paddingHorizontal: 15,
    paddingVertical: 8,
    borderRadius: 999,
    backgroundColor: colors.surfaceRaised,
  },
  chipLabel: { fontSize: 13, color: colors.text, fontWeight: '600' },
  input: {
    minHeight: 48,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
    color: colors.text,
    paddingHorizontal: spacing.md,
    fontSize: 15,
  },
});
