// The "Who's watching?" gate: brand lockup, headline and the wrapping grid of
// profile tiles. Presentation only; every tile's action arrives prebuilt from
// the sign-in screen.

import { ScrollView, StyleSheet } from 'react-native';
import { useT } from '../lib/i18n';
import { spacing } from '../lib/theme';
import { OnboardingBox, OnboardingTitle } from './OnboardingScreen';
import { AddTile, ProfileTile } from './onboarding';
import { ErrorBanner } from './ui';

export type GateTile = {
  key: string;
  name: string;
  caption?: string | null;
  avatarUri: string | null;
  busy?: boolean;
  offline?: boolean;
  locked?: boolean;
  onPress(): void;
};

export function ProfileGate({
  tiles,
  disabled,
  error,
  onAdd,
}: Readonly<{
  tiles: GateTile[];
  /** Freezes every tile while a login is in flight. */
  disabled: boolean;
  error: string | null;
  onAdd(): void;
}>) {
  const t = useT();
  return (
    <OnboardingBox>
      <OnboardingTitle title={t('auth.whoWatching')} />
      <ScrollView contentContainerStyle={styles.grid} style={styles.scroll}>
        {tiles.map((tile) => (
          <ProfileTile
            key={tile.key}
            name={tile.name}
            caption={tile.caption}
            avatarUri={tile.avatarUri}
            busy={tile.busy}
            disabled={disabled}
            offline={tile.offline}
            locked={tile.locked}
            onPress={tile.onPress}
          />
        ))}
        <AddTile label={t('profiles.add')} onPress={onAdd} />
      </ScrollView>
      <ErrorBanner message={error} />
    </OnboardingBox>
  );
}

const styles = StyleSheet.create({
  scroll: { flexGrow: 0 },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'center',
    gap: spacing.lg,
    paddingTop: spacing.md,
    paddingBottom: spacing.sm,
  },
});
