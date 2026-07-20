import { useT } from '@kroma/ui';
import { useNav } from '#tv/app/router';
import { DEVICE_SETTINGS } from '#tv/app/settings/registry';
import { useFocusNav } from '#tv/app/useFocusNav';
import { AuthScreen, KromaMark } from '#tv/shared/ui';
import { SettingsRows } from './SettingsRows';

/**
 * Device settings (route `deviceSettings`), reachable from the signed-out
 * profile picker: the device-level prefs that must not require an account.
 * The rows come straight from the settings registry (DEVICE_SETTINGS);
 * account-level extras live in TvProfileMenu.
 */
export function TvDeviceSettings() {
  const nav = useNav();
  const t = useT();
  useFocusNav({ onBack: nav.back });

  return (
    <AuthScreen>
      <div className="mb-8">
        <KromaMark size={40} />
      </div>
      <h1 className="m-0 mb-9 font-display text-[44px] font-semibold leading-none">
        {t('deviceSettings.title')}
      </h1>

      <div className="flex w-full max-w-[560px] flex-col gap-3">
        <SettingsRows items={DEVICE_SETTINGS} />
      </div>

      <div className="mt-7 font-sans text-[14px] font-medium text-[rgba(244,243,240,0.4)]">
        {t('profileMenu.navHint')}
      </div>
    </AuthScreen>
  );
}
