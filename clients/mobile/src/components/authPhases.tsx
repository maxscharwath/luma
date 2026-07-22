// The inner sign-in phases: the PIN pad unlock and the password / credentials
// form. Presentation only; auth calls and phase switching stay in sign-in.

import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import { useT } from '../lib/i18n';
import { colors, spacing, type } from '../lib/theme';
import { Avatar } from './Avatar';
import { BackLink, OnboardingBox, OnboardingTitle } from './OnboardingScreen';
import { PinPad } from './onboarding';
import { Button, ErrorBanner, TextField } from './ui';

export type Identity = { name: string; avatarUri: string | null };

/** Centered avatar + name header used by the PIN and password phases. */
function IdentityHeader({
  identity,
  subtitle,
}: Readonly<{ identity: Identity; subtitle?: string }>) {
  return (
    <View style={styles.identity}>
      <Avatar uri={identity.avatarUri} name={identity.name} size={72} />
      <Text style={styles.name}>{identity.name}</Text>
      {subtitle ? <Text style={styles.subtitle}>{subtitle}</Text> : null}
    </View>
  );
}

export function PinPhase({
  identity,
  pin,
  disabled,
  checking,
  error,
  onChange,
  onBack,
}: Readonly<{
  identity: Identity;
  pin: string;
  disabled: boolean;
  /** The typed PIN is being verified: shows the small spinner. */
  checking: boolean;
  error: string | null;
  onChange(next: string): void;
  onBack(): void;
}>) {
  const t = useT();
  return (
    <OnboardingBox>
      <IdentityHeader identity={identity} subtitle={t('auth.pinRequired')} />
      <PinPad value={pin} disabled={disabled} onChange={onChange} />
      {checking ? <ActivityIndicator color={colors.textDim} /> : null}
      <ErrorBanner message={error} />
      <BackLink onPress={onBack} />
    </OnboardingBox>
  );
}

export function CredentialsPhase({
  identity,
  serverLabel,
  identifier,
  password,
  busy,
  error,
  onIdentifier,
  onPassword,
  onSubmit,
  onBack,
}: Readonly<{
  /** Known profile (stale-token fallback): avatar header + password only.
   * Null = the full credentials form on the selected server. */
  identity: Identity | null;
  serverLabel: string | null;
  identifier: string;
  password: string;
  busy: boolean;
  error: string | null;
  onIdentifier(next: string): void;
  onPassword(next: string): void;
  onSubmit(): void;
  onBack(): void;
}>) {
  const t = useT();
  return (
    <OnboardingBox>
      {identity ? (
        <IdentityHeader identity={identity} />
      ) : (
        <OnboardingTitle title={t('auth.signinTitle')} subtitle={serverLabel} />
      )}
      {identity ? null : (
        <TextField
          value={identifier}
          onChangeText={onIdentifier}
          placeholder={t('auth.emailOrUsername')}
          keyboardType="email-address"
          textContentType="username"
          autoFocus
        />
      )}
      <TextField
        value={password}
        onChangeText={onPassword}
        placeholder={t('auth.password')}
        secureTextEntry
        textContentType="password"
        autoFocus={identity !== null}
        returnKeyType="go"
        onSubmitEditing={onSubmit}
      />
      <ErrorBanner message={error} />
      <Button
        label={busy ? t('auth.loggingIn') : t('auth.login')}
        onPress={onSubmit}
        loading={busy}
        disabled={!password || (!identity && !identifier.trim())}
      />
      <BackLink onPress={onBack} />
    </OnboardingBox>
  );
}

const styles = StyleSheet.create({
  identity: { alignItems: 'center', gap: spacing.xs, marginBottom: spacing.xs },
  name: { ...type.heading, marginTop: 6 },
  subtitle: { ...type.caption, textAlign: 'center', marginTop: 4 },
});
