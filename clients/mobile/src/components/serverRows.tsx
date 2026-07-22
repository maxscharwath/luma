// The server list visual language shared by the onboarding screens: rows in a
// soft surface card (name + host sub-line + chevron, color-only press
// feedback) plus the uppercase section header and hint that frame them.

import type { ReactNode } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { colors, radius, spacing, type } from '../lib/theme';
import { ChevronRightIcon } from '../player/icons';

/** Soft surface card the rows stack in (the profile-rows card language). */
export function ServerList({ children }: Readonly<{ children: ReactNode }>) {
  return <View style={styles.list}>{children}</View>;
}

export function ServerRow({
  name,
  host,
  icon,
  disabled,
  dimmed,
  onPress,
}: Readonly<{
  name: string;
  /** Optional sub-line under the name (host, or an offline notice). */
  host?: string | null;
  /** Optional leading glyph, wrapped in the soft accent disc. */
  icon?: ReactNode;
  disabled?: boolean;
  /** Faded row (offline server); pairs with `disabled`. */
  dimmed?: boolean;
  onPress(): void;
}>) {
  return (
    <Pressable
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.row,
        pressed && { backgroundColor: colors.surfaceHigh },
        dimmed && { opacity: 0.5 },
      ]}
    >
      {icon ? <View style={styles.leadIcon}>{icon}</View> : null}
      <View style={styles.text}>
        <Text numberOfLines={1} style={styles.name}>
          {name}
        </Text>
        {host ? (
          <Text numberOfLines={1} style={styles.host}>
            {host}
          </Text>
        ) : null}
      </View>
      <ChevronRightIcon size={16} color={colors.textFaint} />
    </Pressable>
  );
}

/** Uppercase section header; `loading` appends a small spinner (live scan). */
export function ServerSectionHeader({
  title,
  loading,
}: Readonly<{ title: string; loading?: boolean }>) {
  return (
    <View style={styles.sectionHeader}>
      <Text style={styles.sectionTitle}>{title}</Text>
      {loading ? <ActivityIndicator size="small" color={colors.textFaint} /> : null}
    </View>
  );
}

export function ServerSectionHint({ children }: Readonly<{ children: string }>) {
  return <Text style={styles.sectionHint}>{children}</Text>;
}

const styles = StyleSheet.create({
  list: {
    backgroundColor: colors.surface,
    borderRadius: radius.lg,
    paddingVertical: 4,
    paddingHorizontal: 6,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    minHeight: 56,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.md,
    gap: spacing.sm,
  },
  text: { flex: 1, gap: 1 },
  name: { ...type.body, color: colors.text, fontWeight: '600' },
  host: { ...type.small, color: colors.textDim },
  leadIcon: {
    width: 30,
    height: 30,
    borderRadius: 15,
    backgroundColor: colors.accentSoft,
    alignItems: 'center',
    justifyContent: 'center',
  },
  sectionHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: spacing.xs,
  },
  sectionTitle: { ...type.small, textTransform: 'uppercase', letterSpacing: 1.2 },
  sectionHint: { ...type.small, paddingHorizontal: spacing.xs },
});
