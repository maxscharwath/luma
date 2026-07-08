// Preferences card: UI language (reuses the shared locale switch, already
// account-synced) plus preferred audio and subtitle languages. The audio/subtitle
// prefs persist on the account (`PATCH /auth/me`) and drive the player's default
// track pick (see useVideoPlayback). Empty value = no preference; subtitles also
// offer the "off" sentinel.

import { LOCALES, langName } from '@luma/core';
import { useLocale, useSetLocale, useT } from '@luma/ui';
import { Card, Select, StatusText, useSave } from '#web/features/accounts/account/ui';
import { useAuth } from '#web/shared/lib/auth';

/** Curated set of offered playback languages the codes with catalog labels. */
const PREF_LANGS = ['en', 'fr', 'es', 'de', 'it', 'pt', 'nl', 'ru', 'ja', 'ko', 'zh'];

/** Sentinel for the "no preference" option Radix Select forbids an empty value,
 * so we map it to `null` (clear the pref) on the way to the server. */
const NONE = 'none';

export function PreferencesCard() {
  const t = useT();
  const locale = useLocale();
  const setLocale = useSetLocale();
  const { user, client, updateUser } = useAuth();
  const audio = useSave();
  const sub = useSave();

  if (!user) return null;

  const langOptions = PREF_LANGS.map((code) => ({ value: code, label: langName(t, code) ?? code }));

  // Keep the current value selectable even if it isn't in the curated list (e.g.
  // a code set on the TV, or a 3-letter code) otherwise Radix shows a blank
  // trigger and the user can't see (or safely keep) their stored preference.
  const withCurrent = (value: string, opts: { value: string; label: string }[]) =>
    opts.some((o) => o.value === value)
      ? opts
      : [...opts, { value, label: langName(t, value) ?? value.toUpperCase() }];

  const setAudio = (value: string) => {
    const audioLanguage = value === NONE ? null : value;
    audio.run(async () => {
      const { user: u } = await client.updateAccount({ audioLanguage });
      updateUser({ audioLanguage: u.audioLanguage ?? null });
    }, t('account.saveFailed'));
  };

  const setSub = (value: string) => {
    const subtitleLanguage = value === NONE ? null : value;
    sub.run(async () => {
      const { user: u } = await client.updateAccount({ subtitleLanguage });
      updateUser({ subtitleLanguage: u.subtitleLanguage ?? null });
    }, t('account.saveFailed'));
  };

  return (
    <Card title={t('account.preferences')} desc={t('account.preferencesSub')}>
      <Select
        label={t('account.uiLanguage')}
        value={locale}
        onChange={(v) => setLocale(v as (typeof LOCALES)[number]['code'])}
        options={LOCALES.map((l) => ({ value: l.code, label: t(l.labelKey) }))}
      />

      <div className="flex flex-col gap-1.5">
        <Select
          label={t('account.audioLanguage')}
          value={user.audioLanguage ?? NONE}
          onChange={setAudio}
          options={withCurrent(user.audioLanguage ?? NONE, [
            { value: NONE, label: t('account.noPreference') },
            ...langOptions,
          ])}
        />
        <StatusText status={audio.status} error={audio.error} />
      </div>

      <div className="flex flex-col gap-1.5">
        <Select
          label={t('account.subtitleLanguage')}
          value={user.subtitleLanguage ?? NONE}
          onChange={setSub}
          options={withCurrent(user.subtitleLanguage ?? NONE, [
            { value: NONE, label: t('account.noPreference') },
            { value: 'off', label: t('player.subtitlesOff') },
            ...langOptions,
          ])}
        />
        <StatusText status={sub.status} error={sub.error} />
      </div>
    </Card>
  );
}
