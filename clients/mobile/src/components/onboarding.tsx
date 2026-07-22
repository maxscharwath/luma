// Onboarding building blocks shared by the sign-in, connect and connect-device
// screens: profile tiles for the "Who's watching?" gate and the segmented
// code cells (PIN pad, Quick Connect code).

import { useRef } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { colors, radius, type } from '../lib/theme';
import { LockIcon, PlusIcon } from '../player/icons';
import { Avatar } from './Avatar';

const TILE_AVATAR = 84;

export function ProfileTile({
  name,
  caption,
  avatarUri,
  busy,
  disabled,
  offline,
  locked,
  onPress,
}: Readonly<{
  name: string;
  caption?: string | null;
  avatarUri: string | null;
  busy?: boolean;
  disabled?: boolean;
  offline?: boolean;
  /** PIN-locked profile: shows a small lock badge on the avatar. */
  locked?: boolean;
  onPress(): void;
}>) {
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || offline}
      style={({ pressed }) => [styles.tile, (pressed || offline) && { opacity: 0.6 }]}
    >
      <View>
        <Avatar uri={avatarUri} name={name} size={TILE_AVATAR} />
        {locked && !busy ? (
          <View style={styles.lockBadge}>
            <LockIcon size={13} color={colors.text} />
          </View>
        ) : null}
        {busy ? (
          <View style={styles.tileBusy}>
            <ActivityIndicator color={colors.text} />
          </View>
        ) : null}
      </View>
      <Text numberOfLines={1} style={styles.tileName}>
        {name}
      </Text>
      {caption ? (
        <Text numberOfLines={1} style={[styles.tileCaption, offline && { color: colors.danger }]}>
          {caption}
        </Text>
      ) : null}
    </Pressable>
  );
}

export function AddTile({ label, onPress }: Readonly<{ label: string; onPress(): void }>) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.tile, pressed && { opacity: 0.6 }]}
    >
      <View style={styles.addCircle}>
        <PlusIcon size={30} color={colors.textDim} />
      </View>
      <Text numberOfLines={1} style={[styles.tileName, { color: colors.textDim }]}>
        {label}
      </Text>
    </Pressable>
  );
}

function cellText(value: string, i: number, masked: boolean): string {
  if (i >= value.length) return '';
  return masked ? '•' : (value[i] ?? '');
}

/** Segmented digit cells over a hidden input; the keyboard stays up and the
 * parent auto-submits once `value` reaches `length`. `masked` renders dots
 * (PIN entry); otherwise the digits show (Quick Connect codes). */
export function CodeCells({
  value,
  onChange,
  length = 4,
  masked = false,
  error = false,
  showActive = true,
  editable = true,
  refocusOnBlur = false,
}: Readonly<{
  value: string;
  onChange(next: string): void;
  length?: number;
  masked?: boolean;
  /** Danger border on every cell (rejected code). */
  error?: boolean;
  /** Accent border on the next cell to fill. */
  showActive?: boolean;
  editable?: boolean;
  /** Pull focus back whenever the hidden input blurs (kiosk-style entry). */
  refocusOnBlur?: boolean;
}>) {
  const inputRef = useRef<TextInput>(null);
  return (
    <Pressable onPress={() => inputRef.current?.focus()} style={styles.pinRow}>
      {Array.from({ length }, (_, i) => (
        <View
          key={`cell-${String(i)}`}
          style={[
            styles.pinCell,
            showActive && i === value.length && styles.pinCellActive,
            error && styles.pinCellError,
          ]}
        >
          <Text style={styles.pinDigit}>{cellText(value, i, masked)}</Text>
        </View>
      ))}
      <TextInput
        ref={inputRef}
        value={value}
        onChangeText={(raw) => onChange(raw.replace(/\D/g, '').slice(0, length))}
        keyboardType="number-pad"
        maxLength={length}
        autoFocus
        editable={editable}
        onBlur={() => {
          if (refocusOnBlur) inputRef.current?.focus();
        }}
        style={styles.pinInput}
        caretHidden
      />
    </Pressable>
  );
}

/** Four masked digit cells for PIN-locked profiles. */
export function PinPad({
  value,
  onChange,
  length = 4,
  disabled,
}: Readonly<{
  value: string;
  onChange(next: string): void;
  length?: number;
  disabled?: boolean;
}>) {
  return (
    <CodeCells value={value} onChange={onChange} length={length} masked editable={!disabled} />
  );
}

const styles = StyleSheet.create({
  tile: { alignItems: 'center', gap: 6, width: 96 },
  tileBusy: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    borderRadius: TILE_AVATAR / 2,
    backgroundColor: 'rgba(10, 10, 12, 0.55)',
    alignItems: 'center',
    justifyContent: 'center',
  },
  lockBadge: {
    position: 'absolute',
    right: -2,
    bottom: -2,
    width: 26,
    height: 26,
    borderRadius: 13,
    backgroundColor: colors.surfaceHigh,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 2.5,
    borderColor: colors.bg,
  },
  tileName: { ...type.caption, color: colors.text, fontWeight: '600', marginTop: 2 },
  tileCaption: { ...type.small, marginTop: -2 },
  addCircle: {
    width: TILE_AVATAR,
    height: TILE_AVATAR,
    borderRadius: TILE_AVATAR / 2,
    borderWidth: 1.5,
    borderColor: colors.borderStrong,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pinRow: { flexDirection: 'row', justifyContent: 'center', gap: 12 },
  pinCell: {
    width: 52,
    height: 60,
    borderRadius: radius.md,
    backgroundColor: colors.surface,
    borderWidth: 1.5,
    borderColor: colors.borderStrong,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pinCellActive: { borderColor: colors.accent },
  pinCellError: { borderColor: colors.danger },
  pinDigit: { fontSize: 30, color: colors.text, fontWeight: '800' },
  pinInput: { position: 'absolute', opacity: 0, width: 1, height: 1 },
});
