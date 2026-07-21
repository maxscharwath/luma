// Every shared user-facing setting, declared ONCE: identity, level, binding and
// presentation. Screens compose menus from these lists (plus screen-local
// actionItems built inline, e.g. the PIN row which needs auth + nav context).
//
// Adding a setting = one declaration here + one entry in a menu list. The row
// UI, the cycle logic, the platform gating and the cross-component reactivity
// all come from items.ts / store.ts / <SettingsRows>.

import { LOCALES } from '@kroma/core';
import { useLocale, useSetLocale } from '@kroma/ui';
import { IconCpu, IconKeyboard, IconLanguage, IconMovie, IconPower } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { canQuitApp, quitApp } from '#tv/app/appQuit';
import { getGpuRendering, gpuToggleAvailable, setGpuRendering } from '#tv/app/desktopGpu';
import { availableEngines, ENGINE_LABEL_KEY, enginePrefStore } from '#tv/app/enginePref';
import {
  ALL_KEYBOARD_LAYOUTS,
  KEYBOARD_LAYOUT_LABEL_KEY,
  keyboardLayoutStore,
} from '#tv/app/keyboardLayoutPref';
import { actionItem, choiceItem, type SettingsItem, toggleItem } from './items';
import { useStoredPref } from './store';

/** Interface language. Account level: the I18nProvider owns the value and the
 * app persists + syncs a change to the signed-in account. */
export const localeSetting: SettingsItem = choiceItem({
  id: 'locale',
  level: 'account',
  label: 'common.language',
  icon: IconLanguage,
  options: () => LOCALES.map((l) => l.code),
  // The find can't miss: options() only offers LOCALES codes.
  valueLabel: (code) => LOCALES.find((l) => l.code === code)?.labelKey ?? 'common.language',
  use: () => [useLocale(), useSetLocale()] as const,
});

/** On-screen keyboard letter order (ABC / AZERTY / QWERTY / QWERTZ). */
export const keyboardLayoutSetting: SettingsItem = choiceItem({
  id: 'keyboardLayout',
  level: 'device',
  label: 'keyboardLayout.title',
  icon: IconKeyboard,
  options: () => ALL_KEYBOARD_LAYOUTS,
  valueLabel: (v) => KEYBOARD_LAYOUT_LABEL_KEY[v],
  use: () => useStoredPref(keyboardLayoutStore),
});

/** Playback engine override. Hides itself on single-engine platforms (the
 * choice-row rule: fewer than two options = no row). */
export const engineSetting: SettingsItem = choiceItem({
  id: 'playbackEngine',
  level: 'device',
  label: 'playbackEngine.title',
  icon: IconMovie,
  options: availableEngines,
  valueLabel: (v) => ENGINE_LABEL_KEY[v],
  use: () => useStoredPref(enginePrefStore),
});

/** Webview GPU renderer, Linux desktop shell only. Shell level: persisted in
 * the shell's config file and applied at boot, so flipping it relaunches. */
export const gpuRenderingSetting: SettingsItem = toggleItem({
  id: 'gpuRendering',
  level: 'shell',
  label: 'profileMenu.gpuRendering',
  icon: IconCpu,
  available: gpuToggleAvailable,
  use: () => {
    const [on, setOn] = useState(false);
    useEffect(() => {
      void getGpuRendering().then(setOn);
    }, []);
    const set = (next: boolean) => {
      setOn(next);
      void setGpuRendering(next); // persists, then relaunches the app
    };
    return [on, set] as const;
  },
});

/** Quit the app - desktop + Android TV shells (fullscreen, no window chrome). */
export const quitAppItem: SettingsItem = actionItem({
  id: 'quitApp',
  label: 'profileMenu.quitApp',
  icon: IconPower,
  available: canQuitApp,
  run: quitApp,
});

/** The signed-out device-settings screen: everything a fresh install needs. */
export const DEVICE_SETTINGS: readonly SettingsItem[] = [
  localeSetting,
  keyboardLayoutSetting,
  gpuRenderingSetting,
  quitAppItem,
];

/** The settings block at the top of the signed-in profile menu. */
export const PROFILE_SETTINGS: readonly SettingsItem[] = [
  localeSetting,
  keyboardLayoutSetting,
  engineSetting,
  gpuRenderingSetting,
];
