// Preferences section: UI language (applied immediately via the shared locale
// switch, already account-synced) plus preferred audio and subtitle languages.
// The audio/subtitle prefs are edited into the parent's pending profile state
// and persist together through the sticky save bar (`PATCH /auth/me`); they
// drive the player's default track pick (see useVideoPlayback). Empty value =
// no preference; subtitles also offer the "off" sentinel.

import { LOCALES, langName } from '@luma/core';
import { useLocale, useSetLocale, useT } from '@luma/ui';
import { IconBadgeCc, IconLanguage, IconVolume } from '@tabler/icons-react';
import { PrefRow } from '#web/features/accounts/account/ui';
import { Select } from '#web/shared/ui';

/** Curated set of offered playback languages the codes with catalog labels. */
const PREF_LANGS = ['en', 'fr', 'es', 'de', 'it', 'pt', 'nl', 'ru', 'ja', 'ko', 'zh'];

/** Sentinel for the "no preference" option Radix Select forbids an empty value,
 * so we map it to `null` (clear the pref) on the way to the server. */
export const NONE = 'none';

const TRIGGER = 'min-w-[min(188px,45vw)]';

export function PreferencesCard({
  audio,
  subtitle,
  onAudio,
  onSubtitle,
}: Readonly<{
  audio: string;
  subtitle: string;
  onAudio: (value: string) => void;
  onSubtitle: (value: string) => void;
}>) {
  const t = useT();
  const locale = useLocale();
  const setLocale = useSetLocale();

  const langOptions = PREF_LANGS.map((code) => ({ value: code, label: langName(t, code) ?? code }));

  // Keep the current value selectable even if it isn't in the curated list (e.g.
  // a code set on the TV, or a 3-letter code) otherwise the trigger shows blank
  // and the user can't see (or safely keep) their stored preference.
  const withCurrent = (value: string, opts: { value: string; label: string }[]) =>
    opts.some((o) => o.value === value)
      ? opts
      : [...opts, { value, label: langName(t, value) ?? value.toUpperCase() }];

  return (
    <div className="divide-y divide-border/70 overflow-visible rounded-xl border border-border bg-surface-1 shadow-card">
      <PrefRow
        icon={<IconLanguage size={18} stroke={1.7} />}
        label={t('account.uiLanguage')}
        desc={t('account.uiLanguageDesc')}
        control={
          <Select
            className={TRIGGER}
            ariaLabel={t('account.uiLanguage')}
            value={locale}
            onChange={(v) => setLocale(v as (typeof LOCALES)[number]['code'])}
            options={LOCALES.map((l) => ({ value: l.code, label: t(l.labelKey) }))}
          />
        }
      />
      <PrefRow
        icon={<IconVolume size={18} stroke={1.7} />}
        label={t('account.audioLanguage')}
        desc={t('account.audioDesc')}
        control={
          <Select
            className={TRIGGER}
            ariaLabel={t('account.audioLanguage')}
            value={audio}
            onChange={onAudio}
            options={withCurrent(audio, [
              { value: NONE, label: t('account.noPreference') },
              ...langOptions,
            ])}
          />
        }
      />
      <PrefRow
        icon={<IconBadgeCc size={18} stroke={1.7} />}
        label={t('account.subtitleLanguage')}
        desc={t('account.subtitleDesc')}
        control={
          <Select
            className={TRIGGER}
            ariaLabel={t('account.subtitleLanguage')}
            value={subtitle}
            onChange={onSubtitle}
            options={withCurrent(subtitle, [
              { value: NONE, label: t('account.noPreference') },
              { value: 'off', label: t('player.subtitlesOff') },
              ...langOptions,
            ])}
          />
        }
      />
    </div>
  );
}
