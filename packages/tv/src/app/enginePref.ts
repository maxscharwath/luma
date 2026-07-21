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
//  - exo      : force the native media3/ExoPlayer engine (Android TV shell).
//  - vlc      : force the native libVLC engine (Android TV shell) - software
//               decode of EVERY codec, the same fallback ExoPlayer hands off to,
//               but made the primary player (the Android equivalent of mpv).

import { isTizenRuntime, isWebOsRuntime, type MessageKey } from '@kroma/core';
import { reactivePref } from '#tv/app/settings/store';
import { exoAvailable, mpvAvailable } from '#tv/features/playback/player/engine';

export type EnginePref = 'auto' | 'avplay' | 'webview' | 'remux' | 'mpv' | 'exo' | 'vlc';

const ALL: readonly EnginePref[] = ['auto', 'avplay', 'webview', 'remux', 'mpv', 'exo', 'vlc'];

/** The reactive store behind the pref (the settings registry binds rows to it). */
export const enginePrefStore = reactivePref('kroma:engine', ALL, 'auto');

/** The saved engine preference for this device, or `auto`. A stored engine no
 * longer offered on THIS platform (e.g. a device left on `remux` before it was
 * retired on Android TV, where the WebView cannot decode HEVC) is degraded to
 * `auto` by the playback engine resolver, not here. */
export function getEnginePref(): EnginePref {
  return enginePrefStore.get();
}

/** Persist the engine preference. */
export function setEnginePref(p: EnginePref): void {
  enginePrefStore.set(p);
}

/** Engines the user may choose on THIS platform (always starts with `auto`), so the
 * menu can offer a real switch. Even the TVs have two players: Tizen can use its
 * native AVPlay OR the HTML5 (`<video>` + hls.js) server-remux path; webOS has no
 * AVPlay but can do direct `<video>` vs the remux. mpv is offered only on the Linux
 * desktop shell. A single-entry list (unknown/other) hides the row. */
export function availableEngines(): EnginePref[] {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  // Android TV: ExoPlayer (hardware) with a libVLC software-decode fallback plays
  // EVERY codec, including the HEVC 10-bit / E-AC3 the Chromium WebView cannot.
  // The `<video>` + hls.js remux path is therefore strictly inferior here (no HEVC
  // decode at all), so it is not offered - it would only ever be a dead end.
  // `vlc` forces that software decoder as the PRIMARY player (like mpv on desktop).
  if (exoAvailable()) return ['auto', 'exo', 'vlc'];
  if (isTizenRuntime(ua)) return ['auto', 'avplay', 'remux'];
  if (isWebOsRuntime(ua)) return ['auto', 'webview', 'remux'];
  const list: EnginePref[] = ['auto', 'webview', 'remux'];
  // mpv is offered when a native mpv engine is present: the Linux/Deck shell (mpv
  // binary), or the macOS shell whose in-process libmpv engine flagged itself.
  if (mpvAvailable()) list.splice(1, 0, 'mpv');
  return list;
}

/** i18n label key for each engine (rendered in the picker). */
export const ENGINE_LABEL_KEY: Record<EnginePref, MessageKey> = {
  auto: 'playbackEngine.auto',
  avplay: 'playbackEngine.avplay',
  webview: 'playbackEngine.webview',
  remux: 'playbackEngine.remux',
  mpv: 'playbackEngine.mpv',
  exo: 'playbackEngine.exo',
  vlc: 'playbackEngine.vlc',
};
