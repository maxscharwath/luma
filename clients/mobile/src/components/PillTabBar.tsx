// Floating liquid-glass tab bar: a centered capsule hovering above the
// content instead of an edge-to-edge bar. Inactive routes are icon-only; the
// focused route expands into its own inner lens with the label. Screens keep
// scrolling underneath (they already pad by TAB_BAR_CLEARANCE).

import { BlurView } from 'expo-blur';
// Type-only deep import: expo-router vendors react-navigation and does not
// re-export the tab bar props type from its root.
import type { BottomTabBarProps } from 'expo-router/build/react-navigation/bottom-tabs';
import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { colors, type } from '../lib/theme';

export function PillTabBar({ state, descriptors, navigation }: Readonly<BottomTabBarProps>) {
  const insets = useSafeAreaInsets();
  return (
    <View
      pointerEvents="box-none"
      style={[styles.dock, { bottom: Math.max(insets.bottom, 12) + 8 }]}
    >
      <View style={styles.shadow}>
        <View style={styles.pill}>
          {Platform.OS === 'ios' ? (
            <BlurView tint="dark" intensity={60} style={StyleSheet.absoluteFill} />
          ) : null}
          {state.routes.map((route, index) => {
            const { options } = descriptors[route.key];
            const focused = state.index === index;
            const label = options.title ?? route.name;
            const color = focused ? colors.accentBright : colors.textFaint;
            const onPress = () => {
              const event = navigation.emit({
                type: 'tabPress',
                target: route.key,
                canPreventDefault: true,
              });
              if (!focused && !event.defaultPrevented) navigation.navigate(route.name);
            };
            return (
              <Pressable
                key={route.key}
                onPress={onPress}
                onLongPress={() => navigation.emit({ type: 'tabLongPress', target: route.key })}
                accessibilityRole="button"
                accessibilityState={{ selected: focused }}
                accessibilityLabel={label}
                style={({ pressed }) => [
                  styles.item,
                  focused && styles.itemActive,
                  pressed && { opacity: 0.7 },
                ]}
              >
                {options.tabBarIcon?.({ focused, color, size: 22 })}
                {focused ? <Text style={[styles.label, { color }]}>{label}</Text> : null}
              </Pressable>
            );
          })}
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  dock: { position: 'absolute', left: 0, right: 0, alignItems: 'center' },
  // Shadow lives on an unclipped wrapper; the pill itself clips the blur.
  shadow: {
    borderRadius: 999,
    shadowColor: '#000',
    shadowOpacity: 0.35,
    shadowRadius: 18,
    shadowOffset: { width: 0, height: 8 },
    elevation: 12,
  },
  pill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    padding: 6,
    borderRadius: 999,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: colors.borderStrong,
    backgroundColor: Platform.OS === 'ios' ? 'rgba(18, 18, 22, 0.55)' : 'rgba(18, 18, 22, 0.96)',
  },
  item: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 7,
    paddingHorizontal: 13,
    paddingVertical: 10,
    borderRadius: 999,
  },
  itemActive: {
    backgroundColor: colors.accentSoft,
    paddingHorizontal: 16,
  },
  label: { ...type.caption, fontSize: 12, fontWeight: '700' },
});
