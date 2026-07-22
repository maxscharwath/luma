// Account tab, kept deliberately simple: identity (tap to edit), one card of
// destinations (downloads / quick connect / settings), quiet sign-out.
// Everything else lives in dedicated pages.

import { useRouter } from 'expo-router';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Avatar } from '../../../components/Avatar';
import { formatBytes, useDownloads } from '../../../lib/downloads';
import { useT } from '../../../lib/i18n';
import { boxed, contentWidth } from '../../../lib/layout';
import { useClient, useSession } from '../../../lib/session';
import { colors, radius, spacing, TAB_BAR_CLEARANCE, type } from '../../../lib/theme';
import {
  ChevronRightIcon,
  DownloadIcon,
  GearIcon,
  LockIcon,
  LogoutIcon,
  PencilIcon,
  TvIcon,
  UsersIcon,
} from '../../../player/icons';

function Row({
  icon,
  label,
  value,
  onPress,
}: Readonly<{
  icon: React.ReactNode;
  label: string;
  value?: string;
  onPress(): void;
}>) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.row, pressed && styles.rowPressed]}
    >
      <View style={styles.rowIconLabel}>
        <View style={styles.rowIconBox}>{icon}</View>
        <Text style={styles.rowLabel}>{label}</Text>
      </View>
      <View style={styles.rowRight}>
        {value ? (
          <Text numberOfLines={1} style={styles.rowValue}>
            {value}
          </Text>
        ) : null}
        <ChevronRightIcon size={16} color={colors.textFaint} />
      </View>
    </Pressable>
  );
}

export default function Profile() {
  const t = useT();
  const router = useRouter();
  const { user, signOut, switchProfile } = useSession();
  const client = useClient();
  const downloads = useDownloads();
  const insets = useSafeAreaInsets();
  const avatar = client.resolveArt(user?.avatarUrl);

  return (
    <ScrollView
      style={styles.screen}
      contentContainerStyle={[styles.body, { paddingTop: insets.top + spacing.xl }]}
    >
      <Pressable
        onPress={() => router.push('/edit-profile' as never)}
        style={({ pressed }) => [styles.identity, pressed && { opacity: 0.85 }]}
      >
        <View>
          <Avatar uri={avatar} name={user?.username} size={96} />
          <View style={styles.editBadge}>
            <PencilIcon size={13} color={colors.accentInk} />
          </View>
        </View>
        <Text style={styles.username}>{user?.username}</Text>
        {user?.email ? <Text style={styles.email}>{user.email}</Text> : null}
      </Pressable>

      <View style={styles.card}>
        <Row
          icon={<UsersIcon size={19} color={colors.accent} />}
          label={t('nav.changeProfile')}
          onPress={() => switchProfile()}
        />
        <Row
          icon={<LockIcon size={19} color={colors.accent} />}
          label={t('account.profileLock')}
          onPress={() => router.push('/profile-pin' as never)}
        />
        <Row
          icon={<DownloadIcon size={19} color={colors.accent} />}
          label={t('offline.downloads')}
          value={downloads.entries.length > 0 ? formatBytes(downloads.totalBytes) : undefined}
          onPress={() => router.push('/downloads' as never)}
        />
        <Row
          icon={<TvIcon size={19} color={colors.accent} />}
          label={t('connect.title')}
          onPress={() => router.push('/connect-device' as never)}
        />
        <Row
          icon={<GearIcon size={19} color={colors.accent} />}
          label={t('nav.settings')}
          onPress={() => router.push('/settings' as never)}
        />
      </View>

      <Pressable
        onPress={() => void signOut()}
        style={({ pressed }) => [styles.signOut, pressed && { opacity: 0.7 }]}
      >
        <LogoutIcon size={18} color={colors.danger} />
        <Text style={styles.signOutText}>{t('auth.logout')}</Text>
      </Pressable>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  body: {
    padding: spacing.md,
    paddingBottom: TAB_BAR_CLEARANCE,
    gap: spacing.md,
    ...boxed(contentWidth.reading),
  },
  identity: { alignItems: 'center', marginBottom: spacing.md, gap: 3 },
  editBadge: {
    position: 'absolute',
    right: -2,
    bottom: -2,
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: colors.accent,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 3,
    borderColor: colors.bg,
  },
  username: { ...type.heading, marginTop: spacing.sm },
  email: { ...type.caption },
  card: {
    backgroundColor: colors.surface,
    borderRadius: radius.lg,
    paddingVertical: 4,
    paddingHorizontal: 6,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 54,
    paddingHorizontal: spacing.sm,
    gap: spacing.md,
    borderRadius: radius.md,
  },
  rowPressed: { backgroundColor: colors.surfaceRaised },
  rowIconLabel: { flexDirection: 'row', alignItems: 'center', gap: 12, flexShrink: 1 },
  rowIconBox: {
    width: 34,
    height: 34,
    borderRadius: 10,
    backgroundColor: colors.accentSoft,
    alignItems: 'center',
    justifyContent: 'center',
  },
  rowLabel: { ...type.body, fontWeight: '500' },
  rowRight: { flexDirection: 'row', alignItems: 'center', gap: 8, flexShrink: 1 },
  rowValue: { ...type.caption, flexShrink: 1 },
  signOut: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    marginTop: spacing.sm,
    minHeight: 46,
  },
  signOutText: { ...type.body, color: colors.danger, fontWeight: '700' },
});
