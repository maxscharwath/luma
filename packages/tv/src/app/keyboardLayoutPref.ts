// User-selectable on-screen keyboard layout, persisted per device (like the
// playback-engine override in enginePref.ts). Surfaced as a cycle-row in the
// profile menu and honored by the OnScreenKeyboard letter grids.
//
//  - abc    : alphabetical rows (the classic TV grid, default).
//  - azerty : French typewriter order.
//  - qwerty : US/UK typewriter order.
//  - qwertz : German/Swiss typewriter order.

import type { MessageKey } from '@kroma/core';
import { devicePref } from '#tv/app/devicePref';

export type KeyboardLayoutPref = 'abc' | 'azerty' | 'qwerty' | 'qwertz';

export const ALL_KEYBOARD_LAYOUTS: readonly KeyboardLayoutPref[] = [
  'abc',
  'azerty',
  'qwerty',
  'qwertz',
];

const PREF = devicePref('kroma:kbd-layout', ALL_KEYBOARD_LAYOUTS, 'abc');

/** The saved keyboard layout for this device, or `abc`. */
export function getKeyboardLayoutPref(): KeyboardLayoutPref {
  return PREF.get();
}

/** Persist the keyboard layout preference. */
export function setKeyboardLayoutPref(p: KeyboardLayoutPref): void {
  PREF.set(p);
}

/** i18n label key for each layout (rendered in the picker). */
export const KEYBOARD_LAYOUT_LABEL_KEY: Record<KeyboardLayoutPref, MessageKey> = {
  abc: 'keyboardLayout.abc',
  azerty: 'keyboardLayout.azerty',
  qwerty: 'keyboardLayout.qwerty',
  qwertz: 'keyboardLayout.qwertz',
};
