// Input-environment capabilities for the shared TV app.
//
// The same @kroma/tv bundle runs on d-pad TVs (Tizen / webOS / Android TV) AND on
// the desktop shell (mouse + physical keyboard, or a Steam Deck gamepad). Design
// stays identical everywhere; only the *input affordances* differ:
//
//  - `pointer`   a fine pointer (mouse, or a webOS magic remote) is present, so
//                the focus ring may follow the cursor (see useFocusNav).
//  - `physicalKeyboard`  a hardware keyboard is present, so text fields render a
//                real, typeable <input>. TV shells have an on-screen keyboard
//                instead and must NOT expose a clickable input (the platform IME
//                the user would summon is not what we want here).
//
// `physicalKeyboard` is keyed off the explicit platform label, NOT off `pointer`:
// a webOS magic remote is a fine pointer yet has no keyboard, so it must keep the
// on-screen keyboard.

import type { ReactNode } from 'react';
import { createContext, useContext, useMemo } from 'react';

export interface TvEnv {
  /** Diagnostics label the shell passes ('Desktop' | 'Tizen' | 'webOS' | 'Android TV' | 'TV'). */
  platform: string;
  /** A fine pointer (mouse / magic remote) is present → let the focus ring track it. */
  pointer: boolean;
  /** A hardware keyboard is present → render real, typeable text inputs instead of the on-screen keyboard. */
  physicalKeyboard: boolean;
}

/** Capability overrides a shell may pass when the platform label alone is wrong
 * (e.g. a Steam Deck is 'Desktop' but gamepad-driven, so `physicalKeyboard:false`). */
export type TvEnvOverrides = Partial<Pick<TvEnv, 'pointer' | 'physicalKeyboard'>>;

function finePointer(): boolean {
  try {
    return typeof matchMedia === 'function' && matchMedia('(pointer: fine)').matches;
  } catch {
    return false;
  }
}

/** Derive the input environment from the platform label, honoring any overrides. */
export function computeEnv(platform: string, overrides: TvEnvOverrides = {}): TvEnv {
  return {
    platform,
    pointer: overrides.pointer ?? finePointer(),
    // Only the desktop shell claims a hardware keyboard; every TV uses its OSK.
    physicalKeyboard: overrides.physicalKeyboard ?? platform === 'Desktop',
  };
}

const EnvContext = createContext<TvEnv>({
  platform: 'TV',
  pointer: false,
  physicalKeyboard: false,
});

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
