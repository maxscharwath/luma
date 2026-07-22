// Presentation for the profile-lock page: the titled section cards, the
// biometric switch row and the step-at-a-time masked PIN wizard. All state
// and auth calls stay in the route (app/(app)/profile-pin.tsx).

import { ActivityIndicator, StyleSheet, Switch, Text, View } from 'react-native';
import { useT } from '../lib/i18n';
import { colors, radius, spacing, type } from '../lib/theme';
import { CodeCells } from './onboarding';
import { Button, ErrorBanner } from './ui';

export function LockCard({
  title,
  sub,
  children,
}: Readonly<{
  title: string;
  sub: string;
  children: React.ReactNode;
}>) {
  return (
    <View style={styles.section}>
      <Text style={styles.sectionTitle}>{title}</Text>
      <View style={styles.card}>
        <Text style={styles.sub}>{sub}</Text>
        {children}
      </View>
    </View>
  );
}

export function BioSwitchRow({
  label,
  value,
  disabled,
  onChange,
}: Readonly<{
  label: string;
  value: boolean;
  disabled: boolean;
  onChange(next: boolean): void;
}>) {
  return (
    <View style={styles.bioRow}>
      <Text style={styles.bioLabel}>{label}</Text>
      <Switch
        value={value}
        disabled={disabled}
        onValueChange={onChange}
        trackColor={{ false: colors.surfaceHigh, true: colors.accent }}
        thumbColor={colors.text}
      />
    </View>
  );
}

/** One masked 4-digit entry step; the parent advances on the fourth digit. */
export function PinWizard({
  subtitle,
  pin,
  busy,
  error,
  onChange,
  onCancel,
}: Readonly<{
  subtitle: string;
  pin: string;
  busy: boolean;
  error: string | null;
  onChange(next: string): void;
  onCancel(): void;
}>) {
  const t = useT();
  return (
    <View style={styles.wizard}>
      <Text style={styles.wizardSub}>{subtitle}</Text>
      <CodeCells value={pin} masked editable={!busy} onChange={onChange} />
      {busy ? <ActivityIndicator color={colors.textDim} /> : null}
      <ErrorBanner message={error} />
      <Button label={t('common.cancel')} kind="ghost" onPress={onCancel} />
    </View>
  );
}

const styles = StyleSheet.create({
  section: { gap: spacing.xs },
  sectionTitle: {
    ...type.small,
    textTransform: 'uppercase',
    letterSpacing: 1,
    marginBottom: 2,
  },
  card: {
    backgroundColor: colors.surface,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: colors.border,
    padding: spacing.md,
    gap: spacing.md,
  },
  sub: { ...type.caption },
  bioRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: spacing.md,
  },
  bioLabel: { ...type.body, fontWeight: '500', flexShrink: 1 },
  wizard: { padding: spacing.md, paddingTop: spacing.xl, gap: spacing.lg },
  wizardSub: { ...type.body, textAlign: 'center', color: colors.textDim },
});
