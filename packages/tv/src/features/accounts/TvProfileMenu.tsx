import { LOCALES } from '@kroma/core';
import { useLocale, useSetLocale, useT } from '@kroma/ui';
import {
  IconChevronRight,
  IconKeyboard,
  IconLanguage,
  IconLock,
  IconLogout,
  IconMovie,
  IconTrash,
  IconUsersGroup,
} from '@tabler/icons-react';
import { type ReactNode, useState } from 'react';
import {
  availableEngines,
  ENGINE_LABEL_KEY,
  type EnginePref,
  getEnginePref,
  setEnginePref,
} from '#tv/app/enginePref';
import {
  ALL_KEYBOARD_LAYOUTS,
  getKeyboardLayoutPref,
  KEYBOARD_LAYOUT_LABEL_KEY,
  type KeyboardLayoutPref,
  setKeyboardLayoutPref,
} from '#tv/app/keyboardLayoutPref';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { AuthScreen, ProfileAvatar } from '#tv/shared/ui';

const MENU_ROW =
  'flex w-full items-center gap-4 rounded-[15px] border border-border bg-[rgba(255,255,255,0.03)] px-5 py-4 text-left outline-none transition-transform focus:scale-[1.02] focus:border-accent';

/** Profile menu (route `profileMenu`): the signed-in account's settings
 * language, PIN, switch profile, sign out, and forget-this-server. */
export function TvProfileMenu() {
  const nav = useNav();
  const t = useT();
  const locale = useLocale();
  const setLocale = useSetLocale();
  const { activeServerUrl, forgetServer, client } = useConnection();
  const { user, switchProfile, logout, forget } = useAuth();
  useFocusNav({ onBack: nav.back });

  // Playback engine override (auto / direct <video> / server remux / mpv). Shown only
  // where there is a choice - native-decoder TVs return just ['auto']. This useState
  // MUST stay ABOVE the `!user` early return: a hook after a conditional return breaks
  // the rules of hooks and crashes with React #300 the moment the profile is switched
  // out (user -> null) - which was the switch-profile black screen.
  const engines = availableEngines();
  const [engine, setEngine] = useState<EnginePref>(getEnginePref);
  const cycleEngine = () => {
    const i = engines.indexOf(engine);
    const next = engines[(i + 1) % engines.length];
    if (!next) return;
    setEngine(next);
    setEnginePref(next);
  };

  // On-screen keyboard layout (ABC / AZERTY / QWERTY / QWERTZ), device-persisted.
  // Same hooks-above-early-return rule as the engine useState.
  const [kbLayout, setKbLayout] = useState<KeyboardLayoutPref>(getKeyboardLayoutPref);
  const cycleKbLayout = () => {
    const i = ALL_KEYBOARD_LAYOUTS.indexOf(kbLayout);
    const next = ALL_KEYBOARD_LAYOUTS[(i + 1) % ALL_KEYBOARD_LAYOUTS.length];
    if (!next) return;
    setKbLayout(next);
    setKeyboardLayoutPref(next);
  };

  if (!user) return null;

  const cycleLocale = () => {
    const i = LOCALES.findIndex((l) => l.code === locale);
    const next = LOCALES[(i + 1) % LOCALES.length];
    if (!next) return;
    setLocale(next.code);
  };
  const localeLabel = LOCALES.find((l) => l.code === locale)?.labelKey;

  const onForgetServer = () => {
    if (activeServerUrl) {
      switchProfile();
      forgetServer(activeServerUrl);
    }
  };
  const onSignOut = () => {
    if (activeServerUrl) forget(user.id, activeServerUrl);
    else void logout();
  };

  return (
    <AuthScreen>
      <div className="mb-8 flex flex-col items-center gap-3.5">
        <ProfileAvatar
          name={user.username}
          seed={user.id}
          size={96}
          radius={26}
          src={client?.resolveArt(user.avatarUrl)}
        />
        <h1 className="m-0 font-display text-[32px] font-semibold">{user.username}</h1>
      </div>

      <div className="flex w-full max-w-[560px] flex-col gap-3">
        <MenuRow
          icon={<IconLanguage size={22} stroke={1.7} />}
          label={t('common.language')}
          onAct={cycleLocale}
        >
          <span className="font-sans text-[16px] font-semibold text-accent">
            {localeLabel ? t(localeLabel) : locale}
          </span>
        </MenuRow>

        <MenuRow
          icon={<IconKeyboard size={22} stroke={1.7} />}
          label={t('keyboardLayout.title')}
          onAct={cycleKbLayout}
        >
          <span className="font-sans text-[16px] font-semibold text-accent">
            {t(KEYBOARD_LAYOUT_LABEL_KEY[kbLayout])}
          </span>
        </MenuRow>

        {engines.length > 1 ? (
          <MenuRow
            icon={<IconMovie size={22} stroke={1.7} />}
            label={t('playbackEngine.title')}
            onAct={cycleEngine}
          >
            <span className="font-sans text-[16px] font-semibold text-accent">
              {t(ENGINE_LABEL_KEY[engine])}
            </span>
          </MenuRow>
        ) : null}

        {user.hasPin ? (
          <MenuRow
            icon={<IconLock size={22} stroke={1.7} />}
            label={t('profileMenu.removePin')}
            onAct={() => nav.go('pin', { intent: 'clear' })}
          >
            <span className="font-sans text-[15px] font-semibold text-success">
              {t('profileMenu.on')}
            </span>
          </MenuRow>
        ) : (
          <MenuRow
            icon={<IconLock size={22} stroke={1.7} />}
            label={t('profileMenu.setPin')}
            onAct={() => nav.go('pin', { intent: 'set' })}
          >
            <span className="font-sans text-[15px] font-semibold text-dim">
              {t('profileMenu.off')}
            </span>
          </MenuRow>
        )}

        <MenuRow
          icon={<IconUsersGroup size={22} stroke={1.7} />}
          label={t('nav.changeProfile')}
          onAct={switchProfile}
        />
        <MenuRow
          icon={<IconLogout size={22} stroke={1.7} />}
          label={t('auth.logout')}
          onAct={onSignOut}
        />
        <MenuRow
          icon={<IconTrash size={22} stroke={1.7} />}
          label={t('profileMenu.forgetServer')}
          onAct={onForgetServer}
          danger
        />
      </div>

      <div className="mt-7 font-sans text-[14px] font-medium text-[rgba(244,243,240,0.4)]">
        {t('profileMenu.navHint')}
      </div>
    </AuthScreen>
  );
}

function MenuRow({
  icon,
  label,
  onAct,
  children,
  danger = false,
}: Readonly<{
  icon: ReactNode;
  label: string;
  onAct: () => void;
  children?: ReactNode;
  danger?: boolean;
}>) {
  return (
    <button data-focus="" type="button" onClick={onAct} className={MENU_ROW}>
      <span
        className={`flex h-10.5 w-10.5 flex-none items-center justify-center rounded-xl bg-[rgba(255,255,255,0.06)] ${
          danger ? 'text-danger' : 'text-muted'
        }`}
      >
        {icon}
      </span>
      <span
        className={`flex-1 font-sans text-[18px] font-bold ${danger ? 'text-danger' : 'text-text'}`}
      >
        {label}
      </span>
      {children ?? <IconChevronRight size={20} className="text-dim" />}
    </button>
  );
}
