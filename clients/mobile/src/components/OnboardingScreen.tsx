// Shared scaffold for the onboarding/login surfaces (sign-in, connect and
// connect-device). ONE brand anchor: the KROMA lockup, same size and same
// position on every screen and every phase; content swaps beneath it with no
// motion, so steps feel like one continuous surface instead of new pages.

import { LinearGradient } from 'expo-linear-gradient';
import type { ReactNode } from 'react';
import {
  KeyboardAvoidingView,
  type KeyboardAvoidingViewProps,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useT } from '../lib/i18n';
import { boxed, contentWidth, useIsWide } from '../lib/layout';
import { colors, SHADE, spacing, type } from '../lib/theme';
import { KromaLockup } from './KromaLockup';

/** Full-screen scaffold: ink background, amber wash over the top, the fixed
 * lockup anchor, keyboard avoidance and safe-area padding. */
export function OnboardingScreen({
  keyboardBehavior,
  children,
}: Readonly<{
  /** Android KeyboardAvoidingView behavior override; iOS always pads. */
  keyboardBehavior?: KeyboardAvoidingViewProps['behavior'];
  children: ReactNode;
}>) {
  const insets = useSafeAreaInsets();
  const wide = useIsWide();
  return (
    <View style={styles.screen}>
      <LinearGradient
        colors={[colors.accentSoft, SHADE.transparent]}
        style={styles.wash}
        pointerEvents="none"
      />
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : keyboardBehavior}
        style={styles.body}
      >
        {/* KeyboardAvoidingView owns its own bottom padding, so the safe-area
            spacing lives on an inner view it never touches. */}
        <View
          style={[
            styles.inner,
            wide && styles.innerCentered,
            {
              paddingTop: insets.top + (wide ? 16 : 56),
              paddingBottom: insets.bottom + 16,
            },
          ]}
        >
          <View style={styles.brand}>
            <KromaLockup height={36} />
          </View>
          {children}
        </View>
      </KeyboardAvoidingView>
    </View>
  );
}

/** The content column under the anchor. In narrow windows it is top-aligned
 * so the headline sits at the exact same y on every phase; a <BackLink> inside
 * pins to the bottom. In wide windows the lockup + box group centers
 * vertically instead, with a fixed minHeight so the anchor still lands at the
 * same y per phase. */
export function OnboardingBox({ children }: Readonly<{ children: ReactNode }>) {
  const wide = useIsWide();
  return <View style={wide ? styles.boxWide : styles.box}>{children}</View>;
}

/** The shared headline (+ optional caption subtitle) typography. */
export function OnboardingTitle({
  title,
  subtitle,
}: Readonly<{ title: string; subtitle?: string | null }>) {
  return (
    <View style={styles.titleBlock}>
      <Text style={styles.headline}>{title}</Text>
      {subtitle ? <Text style={styles.subtitle}>{subtitle}</Text> : null}
    </View>
  );
}

/** The quiet "Retour" action, pinned to the bottom of the content column. */
export function BackLink({ onPress }: Readonly<{ onPress(): void }>) {
  const t = useT();
  return (
    <Pressable
      onPress={onPress}
      hitSlop={8}
      style={({ pressed }) => [styles.backLink, pressed && { opacity: 0.6 }]}
    >
      <Text style={styles.backLinkText}>{t('common.back')}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  wash: { position: 'absolute', top: 0, left: 0, right: 0, height: '40%' },
  body: { flex: 1 },
  inner: {
    flex: 1,
    paddingHorizontal: spacing.lg,
    ...boxed(contentWidth.form),
  },
  innerCentered: { justifyContent: 'center' },
  brand: { alignItems: 'center', marginBottom: 48 },
  box: { flex: 1, gap: spacing.md },
  boxWide: { minHeight: 320, gap: spacing.md },
  titleBlock: { marginBottom: spacing.sm },
  headline: { ...type.display, fontSize: 28, textAlign: 'center' },
  subtitle: { ...type.caption, textAlign: 'center', marginTop: 6 },
  backLink: { alignSelf: 'center', padding: spacing.sm, marginTop: 'auto' },
  backLinkText: { ...type.caption, color: colors.textDim, fontWeight: '600' },
});
