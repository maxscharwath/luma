// User-selectable playback engine (a manual override of the automatic
// `selectEngine` decision), persisted per device. Surfaced as a cycle-row in the
// profile menu and honored by `useDirectPlayback`.
//
//  - auto     : let selectEngine + the platform decide (default).
//  - avplay   : force Samsung's native AVPlay (Tizen only) - hardware decode +
//               surround passthrough, plays the original file directly.
//  - webview  : force the in-page `<video>` direct-play (WKWebView = VideoToolbox on
//               macOS; a bare `<video src>` at the original file, zero server work).
//  - remux    : force the server HLS master through `<video>` + hls.js (works for
//               anything the server can remux, incl. MKV; video is stream-copied).
//  - mpv      : force the native mpv engine (VA-API on the Linux/Deck shell).

import type { MessageKey } from '@luma/core';
import { getTauri } from '#tv/features/playback/player/engine';

export type EnginePref = 'auto' | 'avplay' | 'webview' | 'remux' | 'mpv';

const KEY = 'luma:engine';

const ALL: readonly EnginePref[] = ['auto', 'avplay', 'webview', 'remux', 'mpv'];

/** The saved engine preference for this device, or `auto`. */
export function getEnginePref(): EnginePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v && (ALL as readonly string[]).includes(v)) return v as EnginePref;
  } catch {
    /* storage unavailable */
  }
  return 'auto';
}

/** Persist the engine preference. */
export function setEnginePref(p: EnginePref): void {
  try {
    localStorage.setItem(KEY, p);
  } catch {
    /* storage unavailable */
  }
}

/** Engines the user may choose on THIS platform (always starts with `auto`), so the
 * menu can offer a real switch. Even the TVs have two players: Tizen can use its
 * native AVPlay OR the HTML5 (`<video>` + hls.js) server-remux path; webOS has no
 * AVPlay but can do direct `<video>` vs the remux. mpv is offered only on the Linux
 * desktop shell. A single-entry list (unknown/other) hides the row. */
export function availableEngines(): EnginePref[] {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  if (/tizen/i.test(ua)) return ['auto', 'avplay', 'remux'];
  if (/web0?s/i.test(ua)) return ['auto', 'webview', 'remux'];
  const list: EnginePref[] = ['auto', 'webview', 'remux'];
  const isLinux = /Linux/i.test(ua) && !/Android/i.test(ua);
  if (getTauri() != null && isLinux) list.splice(1, 0, 'mpv');
  return list;
}

/** i18n label key for each engine (rendered in the picker). */
export const ENGINE_LABEL_KEY: Record<EnginePref, MessageKey> = {
  auto: 'playbackEngine.auto',
  avplay: 'playbackEngine.avplay',
  webview: 'playbackEngine.webview',
  remux: 'playbackEngine.remux',
  mpv: 'playbackEngine.mpv',
};
