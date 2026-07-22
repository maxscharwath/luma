// Edit profile: photo, personal information, preferred playback languages and
// password, all synced to the account (same PATCH /auth/me + avatar upload as
// the web client).

import { KromaApiError, langName } from '@kroma/core';
import * as ImagePicker from 'expo-image-picker';
import { useState } from 'react';
import { KeyboardAvoidingView, Platform, ScrollView, StyleSheet, Text, View } from 'react-native';
import { Avatar } from '../../components/Avatar';
import { PageHeader } from '../../components/PageHeader';
import { Button, Chip, Screen, TextField } from '../../components/ui';
import { useT } from '../../lib/i18n';
import { boxed, contentWidth } from '../../lib/layout';
import { useClient, useSession } from '../../lib/session';
import { colors, radius, spacing, type } from '../../lib/theme';

const TRACK_LANGS = [null, 'fr', 'en', 'es', 'de', 'it', 'ja', 'ko'] as const;

function Section({ title, children }: Readonly<{ title: string; children: React.ReactNode }>) {
  return (
    <View style={styles.section}>
      <Text style={styles.sectionTitle}>{title}</Text>
      <View style={styles.card}>{children}</View>
    </View>
  );
}

function LangPicker({
  label,
  value,
  onPick,
}: Readonly<{
  label: string;
  value: string | null;
  onPick(code: string | null): void;
}>) {
  const t = useT();
  return (
    <View style={styles.langBlock}>
      <Text style={styles.fieldLabel}>{label}</Text>
      <View style={styles.langRow}>
        {TRACK_LANGS.map((code) => (
          <Chip
            key={code ?? 'none'}
            label={code ? (langName(t, code) ?? code.toUpperCase()) : t('account.noPreference')}
            active={value === code}
            onPress={() => onPick(code)}
          />
        ))}
      </View>
    </View>
  );
}

export default function EditProfile() {
  const t = useT();
  const client = useClient();
  const { user, setUser } = useSession();

  const [username, setUsername] = useState(user?.username ?? '');
  const [email, setEmail] = useState(user?.email ?? '');
  const [savingInfo, setSavingInfo] = useState(false);
  const [infoMessage, setInfoMessage] = useState<string | null>(null);

  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [savingPassword, setSavingPassword] = useState(false);
  const [passwordMessage, setPasswordMessage] = useState<string | null>(null);

  const [avatarBusy, setAvatarBusy] = useState(false);

  const avatar = client.resolveArt(user?.avatarUrl);

  const pickPhoto = async () => {
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ['images'],
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.9,
    });
    const asset = result.assets?.[0];
    if (result.canceled || !asset) return;
    setAvatarBusy(true);
    setInfoMessage(null);
    try {
      const blob = await (await fetch(asset.uri)).blob();
      const { avatarUrl } = await client.uploadAvatar(blob);
      if (user) setUser({ ...user, avatarUrl });
    } catch {
      setInfoMessage(t('account.avatarFailed'));
    } finally {
      setAvatarBusy(false);
    }
  };

  const saveInfo = async () => {
    setSavingInfo(true);
    setInfoMessage(null);
    try {
      const { user: updated } = await client.updateAccount({
        username: username.trim(),
        email: email.trim(),
      });
      setUser(updated);
      setInfoMessage(t('account.profileSaved'));
    } catch (err) {
      if (err instanceof KromaApiError && err.status === 409) setInfoMessage(t('auth.emailTaken'));
      else setInfoMessage(t('account.saveFailed'));
    } finally {
      setSavingInfo(false);
    }
  };

  const savePref = async (patch: {
    audioLanguage?: string | null;
    subtitleLanguage?: string | null;
  }) => {
    try {
      const { user: updated } = await client.updateAccount(patch);
      setUser(updated);
    } catch {
      // Preference sync is best-effort; the UI reflects the server state.
    }
  };

  const savePassword = async () => {
    if (next !== confirm) {
      setPasswordMessage(t('account.passwordMismatch'));
      return;
    }
    setSavingPassword(true);
    setPasswordMessage(null);
    try {
      await client.changePassword(current, next);
      setCurrent('');
      setNext('');
      setConfirm('');
      setPasswordMessage(t('account.profileSaved'));
    } catch {
      setPasswordMessage(t('account.saveFailed'));
    } finally {
      setSavingPassword(false);
    }
  };

  return (
    <Screen padded={false}>
      <PageHeader title={t('account.title')} />
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        style={{ flex: 1 }}
      >
        <ScrollView contentContainerStyle={styles.body} keyboardShouldPersistTaps="handled">
          <Section title={t('account.sectionPhoto')}>
            <View style={styles.avatarRow}>
              <Avatar uri={avatar} name={user?.username} size={88} />
              <View style={{ flex: 1, gap: 6 }}>
                <Button
                  label={t('account.changePhoto')}
                  kind="ghost"
                  onPress={() => void pickPhoto()}
                  loading={avatarBusy}
                />
                <Text style={styles.hint}>{t('account.photoHint')}</Text>
              </View>
            </View>
          </Section>

          <Section title={t('account.sectionInfo')}>
            <View style={styles.fields}>
              <Text style={styles.fieldLabel}>{t('auth.username')}</Text>
              <TextField value={username} onChangeText={setUsername} />
              <Text style={styles.fieldLabel}>{t('auth.email')}</Text>
              <TextField value={email} onChangeText={setEmail} keyboardType="email-address" />
              <Button
                label={t('common.save')}
                onPress={() => void saveInfo()}
                loading={savingInfo}
                disabled={!username.trim() || !email.trim()}
              />
              {infoMessage ? <Text style={styles.message}>{infoMessage}</Text> : null}
            </View>
          </Section>

          <Section title={t('account.sectionPrefs')}>
            <LangPicker
              label={t('account.audioLanguage')}
              value={user?.audioLanguage ?? null}
              onPick={(code) => void savePref({ audioLanguage: code })}
            />
            <LangPicker
              label={t('account.subtitleLanguage')}
              value={user?.subtitleLanguage ?? null}
              onPick={(code) => void savePref({ subtitleLanguage: code })}
            />
          </Section>

          <Section title={t('account.sectionSecurity')}>
            <View style={styles.fields}>
              <Text style={styles.fieldLabel}>{t('account.currentPassword')}</Text>
              <TextField value={current} onChangeText={setCurrent} secureTextEntry />
              <Text style={styles.fieldLabel}>{t('account.newPassword')}</Text>
              <TextField value={next} onChangeText={setNext} secureTextEntry />
              <Text style={styles.fieldLabel}>{t('account.confirmPassword')}</Text>
              <TextField value={confirm} onChangeText={setConfirm} secureTextEntry />
              <Button
                label={t('account.updatePassword')}
                kind="ghost"
                onPress={() => void savePassword()}
                loading={savingPassword}
                disabled={!current || next.length < 4}
              />
              {passwordMessage ? <Text style={styles.message}>{passwordMessage}</Text> : null}
            </View>
          </Section>
        </ScrollView>
      </KeyboardAvoidingView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  body: {
    padding: spacing.md,
    paddingBottom: spacing.xl * 2,
    gap: spacing.lg,
    ...boxed(contentWidth.form),
  },
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
  avatarRow: { flexDirection: 'row', alignItems: 'center', gap: spacing.md },
  hint: { ...type.small },
  fields: { gap: 10 },
  fieldLabel: { ...type.caption, marginTop: 2 },
  message: { ...type.caption, color: colors.accent, textAlign: 'center' },
  langBlock: { gap: 8 },
  langRow: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
});
