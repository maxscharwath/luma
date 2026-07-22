// Whole-surface states: what a screen shows instead of content while it is
// loading, empty, or broken, plus the inline error banner forms use.

import { type ReactNode, useEffect, useRef } from 'react';
import { ActivityIndicator, Animated, StyleSheet, Text, View } from 'react-native';
import { colors, radius, spacing, type } from '../../lib/theme';
import { AlertIcon } from '../../player/icons';
import { Button } from './controls';

/** Inline error surface: tinted banner with an icon and a small shake when the
 * message appears or changes, so failures read at a glance instead of as a
 * bare red caption. Renders nothing while `message` is null. */
export function ErrorBanner({ message }: Readonly<{ message: string | null }>) {
  const shake = useRef(new Animated.Value(0)).current;
  const fade = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    if (!message) return;
    fade.setValue(0);
    shake.setValue(0);
    Animated.parallel([
      Animated.timing(fade, { toValue: 1, duration: 140, useNativeDriver: true }),
      Animated.sequence(
        [8, -7, 5, -3, 0].map((toValue) =>
          Animated.timing(shake, { toValue, duration: 55, useNativeDriver: true }),
        ),
      ),
    ]).start();
  }, [message, fade, shake]);
  if (!message) return null;
  return (
    <Animated.View style={[styles.box, { opacity: fade, transform: [{ translateX: shake }] }]}>
      <AlertIcon size={17} color={colors.danger} />
      <Text style={styles.boxText}>{message}</Text>
    </Animated.View>
  );
}

export function Loading({ label }: Readonly<{ label?: string }>) {
  return (
    <View style={styles.center}>
      <ActivityIndicator color={colors.textDim} size="large" />
      {label ? <Text style={styles.centerText}>{label}</Text> : null}
    </View>
  );
}

export function ErrorView({
  message,
  retryLabel,
  onRetry,
}: Readonly<{
  message: string;
  retryLabel?: string;
  onRetry?: () => void;
}>) {
  return (
    <View style={styles.center}>
      <Text style={styles.centerText}>{message}</Text>
      {onRetry && retryLabel ? (
        <View style={styles.retry}>
          <Button label={retryLabel} onPress={onRetry} kind="ghost" />
        </View>
      ) : null}
    </View>
  );
}

/** Clean centered empty state: icon in a soft disc, title, hint, optional CTA. */
export function EmptyState({
  icon,
  title,
  hint,
  actionLabel,
  onAction,
}: Readonly<{
  icon: ReactNode;
  title: string;
  hint?: string;
  actionLabel?: string;
  onAction?: () => void;
}>) {
  return (
    <View style={styles.emptyBox}>
      <View style={styles.emptyDisc}>{icon}</View>
      <Text style={styles.emptyTitle}>{title}</Text>
      {hint ? <Text style={styles.emptyHint}>{hint}</Text> : null}
      {actionLabel && onAction ? (
        <View style={styles.emptyAction}>
          <Button label={actionLabel} kind="ghost" onPress={onAction} />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  box: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    backgroundColor: 'rgba(229, 57, 53, 0.12)',
    borderWidth: 1,
    borderColor: 'rgba(229, 57, 53, 0.35)',
    borderRadius: radius.md,
    paddingHorizontal: spacing.sm,
    paddingVertical: 10,
  },
  boxText: { ...type.caption, color: '#FF8A80', flex: 1, fontWeight: '600' },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: spacing.lg },
  centerText: { ...type.caption, textAlign: 'center' },
  retry: { marginTop: spacing.md, minWidth: 160 },
  emptyBox: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: spacing.xl,
    // Optical centering: a mathematically centered block reads too low, so
    // bias it upward toward the ~45% line.
    paddingBottom: 150,
    gap: 6,
  },
  emptyDisc: {
    width: 84,
    height: 84,
    borderRadius: 42,
    backgroundColor: colors.surfaceRaised,
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: spacing.sm,
  },
  emptyTitle: { ...type.section, textAlign: 'center' },
  emptyHint: { ...type.caption, textAlign: 'center', maxWidth: 300, lineHeight: 20 },
  emptyAction: { marginTop: spacing.md, minWidth: 180 },
});
