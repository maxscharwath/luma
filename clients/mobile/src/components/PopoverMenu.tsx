// Anchored popover menu (iOS-menu style): an elevated card that springs open
// from the trigger's position, with a press-through backdrop. Pure RN Animated,
// reusable for any small option list (seasons, sort, ...).

import { useEffect, useRef } from 'react';
import {
  Animated,
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  useWindowDimensions,
  View,
} from 'react-native';
import { colors, radius, spacing, type } from '../lib/theme';
import { CheckIcon } from '../player/icons';

export interface PopoverAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PopoverItem {
  key: string;
  label: string;
  detail?: string;
  active?: boolean;
  onPress(): void;
}

const MENU_WIDTH = 250;

export function PopoverMenu({
  visible,
  anchor,
  items,
  onClose,
}: Readonly<{
  visible: boolean;
  anchor: PopoverAnchor | null;
  items: PopoverItem[];
  onClose(): void;
}>) {
  const { width: screenW, height: screenH } = useWindowDimensions();
  const scale = useRef(new Animated.Value(0.85)).current;
  const opacity = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    if (visible) {
      scale.setValue(0.85);
      opacity.setValue(0);
      Animated.parallel([
        Animated.spring(scale, {
          toValue: 1,
          useNativeDriver: true,
          stiffness: 320,
          damping: 24,
          mass: 0.8,
        }),
        Animated.timing(opacity, { toValue: 1, duration: 120, useNativeDriver: true }),
      ]).start();
    }
  }, [visible, scale, opacity]);

  if (!anchor) return null;
  const left = Math.max(spacing.md, Math.min(anchor.x, screenW - MENU_WIDTH - spacing.md));
  const below = anchor.y + anchor.height + 8;
  const maxHeight = Math.min(items.length * 50 + 16, screenH * 0.5);
  // Flip above the trigger when there is no room below.
  const top = below + maxHeight > screenH - 40 ? Math.max(40, anchor.y - maxHeight - 8) : below;

  return (
    <Modal visible={visible} transparent animationType="none" onRequestClose={onClose}>
      <Pressable style={styles.backdrop} onPress={onClose} />
      <Animated.View
        style={[
          styles.menu,
          {
            left,
            top,
            maxHeight,
            opacity,
            transform: [{ scale }],
          },
        ]}
      >
        <ScrollView showsVerticalScrollIndicator={false}>
          {items.map((item) => (
            <Pressable
              key={item.key}
              onPress={() => {
                item.onPress();
                onClose();
              }}
              style={({ pressed }) => [
                styles.row,
                pressed && { backgroundColor: colors.surfaceHigh },
              ]}
            >
              <Text style={[styles.label, item.active && styles.labelActive]}>{item.label}</Text>
              <View style={styles.right}>
                {item.detail ? <Text style={styles.detail}>{item.detail}</Text> : null}
                {item.active ? <CheckIcon size={16} color={colors.accent} /> : null}
              </View>
            </Pressable>
          ))}
        </ScrollView>
      </Animated.View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: { flex: 1, backgroundColor: 'rgba(0, 0, 0, 0.25)' },
  menu: {
    position: 'absolute',
    width: MENU_WIDTH,
    backgroundColor: colors.surfaceRaised,
    borderRadius: radius.lg,
    paddingVertical: 6,
    paddingHorizontal: 6,
    transformOrigin: 'top left',
    shadowColor: '#000',
    shadowOpacity: 0.45,
    shadowRadius: 24,
    shadowOffset: { width: 0, height: 12 },
    elevation: 12,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 46,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.sm,
    gap: spacing.sm,
  },
  label: { ...type.body, color: colors.text },
  labelActive: { fontWeight: '800', color: colors.accent },
  right: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  detail: { ...type.small },
});
