// Input-environment capabilities for the shared TV app.
//
// The same @kroma/tv bundle runs on d-pad TVs (Tizen / webOS / Android TV) AND on
// the desktop shell (mouse + physical keyboard, or a Steam Deck gamepad). Design
// stays identical everywhere; only the *input affordances* differ:
//
//  - `mousePointer`  a REAL mouse (the desktop shell) is present, so pointer-move
//                UX like the player's reveal-on-move arms. Focus itself never
//                follows the cursor: the ring moves on D-pad / arrows only.
//  - `physicalKeyboard`  a hardware keyboard is present, so text fields render a
//                real, typeable <input>. TV shells have an on-screen keyboard
//                instead and must NOT expose a clickable input (the platform IME
//                the user would summon is not what we want here).
//
// Both are keyed off the explicit platform label, NOT off the pointer media query
// alone: a webOS magic remote reports `(pointer: fine)` yet has no keyboard and
// emits phantom pointermove events, so it must keep the on-screen keyboard and
// stay out of the mouse-driven paths.

import { isTizenRuntime, isWebOsRuntime } from '@kroma/core';
import type { ReactNode } from 'react';
import { createContext, useContext, useMemo } from 'react';

export interface TvEnv {
  /** Diagnostics label the shell passes ('Desktop' | 'Tizen' | 'webOS' | 'Android TV' | 'TV'). */
  platform: string;
  /** A REAL mouse (desktop shell), not a magic-remote fine pointer. Keyed off the
   * platform like `physicalKeyboard`: a webOS/Tizen magic remote reports
   * `(pointer: fine)` yet emits phantom pointermove events, so only a Desktop
   * mouse should drive pointer-move UX like the player's reveal-on-move. */
  mousePointer: boolean;
  /** A hardware keyboard is present → render real, typeable text inputs instead of the on-screen keyboard. */
  physicalKeyboard: boolean;
}

/** Capability overrides a shell may pass when the platform label alone is wrong
 * (e.g. a Steam Deck is 'Desktop' but gamepad-driven, so `physicalKeyboard:false`). */
export interface TvEnvOverrides {
  /** Force the fine-pointer probe (what `mousePointer` is derived from). */
  pointer?: boolean;
  /** Force hardware-keyboard text entry on/off. */
  physicalKeyboard?: boolean;
}

function finePointer(): boolean {
  try {
    return typeof matchMedia === 'function' && matchMedia('(pointer: fine)').matches;
  } catch {
    return false;
  }
}

/** Runtime probes for the actual TV platforms, keyed by shell label. Tizen and
 * webOS come from @kroma/core so the whole codebase sniffs them one way (the
 * webOS UA has two spellings and a global bridge); the Android TV shell is a
 * plain Android webview, so its UA is all there is to go on. */
const TV_RUNTIME: Record<string, (ua: string) => boolean> = {
  Tizen: isTizenRuntime,
  webOS: isWebOsRuntime,
  'Android TV': (ua) => /android/i.test(ua),
};

/** True only when a TV-shell build is really running on its TV: the platform
 * label says which shell was built, the user agent says where it executes. A
 * Tizen bundle previewed in desktop Chrome (the dev shell) has no "Tizen" UA,
 * so it keeps desktop input affordances. */
function onRealTv(platform: string): boolean {
  const probe = TV_RUNTIME[platform];
  if (!probe) return false;
  try {
    return probe(navigator.userAgent);
  } catch {
    return true; // no navigator: assume the TV, keep the OSK
  }
}

/** Derive the input environment from the platform label, honoring any overrides. */
export function computeEnv(platform: string, overrides: TvEnvOverrides = {}): TvEnv {
  const pointer = overrides.pointer ?? finePointer();
  return {
    platform,
    // A real mouse only on the desktop shell; a TV magic remote is a fine pointer
    // but must not drive pointer-move UX.
    mousePointer: pointer && platform === 'Desktop',
    // Hardware-keyboard input everywhere EXCEPT on an actual TV: remotes must
    // never double-drive text entry, but the same TV bundles previewed in a
    // desktop browser (dev shells) type freely. A shell can still override,
    // e.g. the Steam Deck passes physicalKeyboard:false.
    physicalKeyboard: overrides.physicalKeyboard ?? !onRealTv(platform),
  };
}

// The value seen when no <EnvProvider> is mounted (a stray render, a unit test).
// Built through computeEnv so it can never drift from the real derivation, with
// the conservative overrides spelled out: an unknown host is assumed to be a
// remote-driven TV, so text entry keeps the on-screen keyboard and nothing
// mouse-driven arms. (computeEnv('TV') alone would say `physicalKeyboard: true`,
// which is right for a dev preview but wrong as a blind default.)
const EnvContext = createContext<TvEnv>(
  computeEnv('TV', { pointer: false, physicalKeyboard: false }),
);

export function EnvProvider({
  platform,
  overrides,
  children,
}: Readonly<{ platform: string; overrides?: TvEnvOverrides; children: ReactNode }>) {
  const value = useMemo(() => computeEnv(platform, overrides), [platform, overrides]);
  return <EnvContext.Provider value={value}>{children}</EnvContext.Provider>;
}

/** Read the current input environment (pointer / keyboard capabilities). */
export function useEnv(): TvEnv {
  return useContext(EnvContext);
}
