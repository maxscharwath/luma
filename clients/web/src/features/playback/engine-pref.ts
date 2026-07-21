// The web player's manual engine override, persisted per device (mirrors the TV
// shell's `enginePref`, but web has fewer real pipelines so the choices are
// fewer). `auto` lets the runtime-cap based `selectEngine` decide (direct-play
// when it can, else the HLS master) - and the master plays through Shaka Player
// BY DEFAULT. `direct` forces the bare `<video src>` at the original file (and
// still falls back to the master on a decode error); `remux` forces the master
// through hls.js (the escape hatch if Shaka ever mis-handles a stream); `shaka`
// forces the master through Shaka even for a direct-play-able file.

export type WebEnginePref = 'auto' | 'direct' | 'remux' | 'shaka';

const KEY = 'kroma:web-engine';
const ALL: readonly WebEnginePref[] = ['auto', 'direct', 'remux', 'shaka'];

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
