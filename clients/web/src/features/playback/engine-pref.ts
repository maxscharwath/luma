// The web player's manual engine override, persisted per device (mirrors the TV
// shell's `enginePref`, but web has only two real pipelines so the choices are
// fewer). `auto` lets the runtime-cap based `selectEngine` decide; `direct`
// forces the bare `<video src>` at the original file (and still falls back to the
// remux on a decode error); `remux` forces the server HLS master through hls.js.

export type WebEnginePref = 'auto' | 'direct' | 'remux';

const KEY = 'kroma:web-engine';
const ALL: readonly WebEnginePref[] = ['auto', 'direct', 'remux'];

/** The saved engine preference for this device, or `auto`. */
export function getWebEnginePref(): WebEnginePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v && (ALL as readonly string[]).includes(v)) return v as WebEnginePref;
  } catch {
    /* storage unavailable */
  }
  return 'auto';
}

/** Persist the engine preference. */
export function setWebEnginePref(p: WebEnginePref): void {
  try {
    localStorage.setItem(KEY, p);
  } catch {
    /* storage unavailable */
  }
}
