// A thin playback-engine abstraction for the TV player so the same hook/UI can
// drive either a plain HTML `<video>` (+ hls.js) or Samsung's native AVPlay.
//
// AVPlay decodes AC3/EAC3/DTS in hardware and renders to a video plane behind the
// page, which `<video>`/MSE on Tizen cannot do, so it is the right backend for
// surround passthrough + seamless in-place audio switching. webOS and plain
// compatible MP4 stay on the HTML engine.

/** Normalised lifecycle callbacks the hook subscribes to (absolute seconds). */
export interface EngineListeners {
  onTime(sec: number): void;
  onDuration(sec: number): void;
  onBuffered(sec: number): void;
  onPlay(): void;
  onPause(): void;
  onWaiting(): void;
  onPlaying(): void;
  onEnded(): void;
  onError(): void;
  /** Metadata/decoder ready: safe to apply a resume seek and start playback. */
  onReady(): void;
}

/** The uniform surface the hook + UI talk to, regardless of backend. */
export interface TvEngine {
  readonly kind: 'video' | 'avplay' | 'mpv' | 'exo';
  play(): void;
  pause(): void;
  isPaused(): boolean;
  /** Current position in seconds. */
  position(): number;
  /** Duration in seconds (0 when unknown). */
  duration(): number;
  /** End of the buffered range in seconds. */
  bufferedEnd(): number;
  /** Seek to an ABSOLUTE position in seconds (native + instant on a VOD source). */
  seekTo(absSec: number): void;
  /** Select an audio rendition by its audio-relative index (`0:a:<index>`). */
  setAudioRendition(rendition: number): void;
  destroy(): void;
}

// ----- Tizen AVPlay typings (not in the TS lib; declared loosely) -------------

/** One track from `getTotalTrackInfo()`. `extra_info` is a JSON string. */
export interface AvplayTrack {
  index: number;
  type: 'VIDEO' | 'AUDIO' | 'TEXT' | string;
  extra_info?: string;
}

/** Native AVPlay event callbacks (all optional). */
export interface AvplayListeners {
  onbufferingstart?: () => void;
  onbufferingcomplete?: () => void;
  onbufferingprogress?: (percent: number) => void;
  oncurrentplaytime?: (ms: number) => void;
  onstreamcompleted?: () => void;
  onerror?: (err: unknown) => void;
  onevent?: (type: string, data: unknown) => void;
}

export interface AvplayApi {
  open(url: string): void;
  close(): void;
  prepareAsync(onSuccess: () => void, onError: (e: unknown) => void): void;
  play(): void;
  pause(): void;
  stop(): void;
  seekTo(ms: number, onSuccess?: () => void, onError?: (e: unknown) => void): void;
  getCurrentTime(): number;
  getDuration(): number;
  getState(): string;
  setDisplayRect(x: number, y: number, w: number, h: number): void;
  setStreamingProperty(kind: string, value: string): void;
  getTotalTrackInfo(): AvplayTrack[];
  setSelectTrack(type: 'AUDIO' | 'TEXT' | 'VIDEO', index: number): void;
  setSilentSubtitle(on: boolean): void;
  suspend(): void;
  restore(url: string, ms: number, state: string): void;
  setListener(listeners: AvplayListeners): void;
}

type AvplayGlobal = { webapis?: { avplay?: AvplayApi } };

/** The native AVPlay API when running on a Tizen device, else `null`. */
export function getAvplay(): AvplayApi | null {
  const w = globalThis as unknown as AvplayGlobal;
  return w.webapis?.avplay ?? null;
}

/** Whether to drive playback through native AVPlay (Tizen only). */
export function avplayAvailable(): boolean {
  return getAvplay() != null;
}

// ----- Desktop mpv bridge (Tauri) --------------------------------------------
// The @kroma/desktop shell (a Tauri app, Steam Deck the primary target) runs a
// native mpv process for video (VA-API hardware decode of HEVC + surround audio)
// and exposes a tiny command surface + event stream to the webview. We reach it
// through Tauri's injected `window.__TAURI__` globals (the shell sets
// `app.withGlobalTauri: true`), so @kroma/tv needs no Tauri dependency and this
// whole path stays inert in a plain browser (getTauri() → null → the HTML/AVPlay
// engines are used instead).

/** The slice of Tauri's global API the mpv engine uses. */
export interface TauriBridge {
  core: { invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> };
  event: {
    listen(event: string, cb: (e: { payload: unknown }) => void): Promise<() => void>;
  };
}

/** Tauri's injected global API when running inside the Steam Deck shell, else null. */
export function getTauri(): TauriBridge | null {
  const w = globalThis as unknown as { __TAURI__?: Partial<TauriBridge> };
  const t = w.__TAURI__;
  return t?.core?.invoke && t?.event?.listen ? (t as TauriBridge) : null;
}

/** Whether to drive playback through the native mpv process. Only the LINUX desktop
 * shell spawns mpv (the Deck's VA-API path); on macOS the WKWebView decodes HEVC via
 * VideoToolbox, so there we use the in-page `<video>` engine and never spawn a second
 * (mpv) window. So mpv is gated to a Tauri shell running on Linux. */
export function mpvAvailable(): boolean {
  if (getTauri() == null) return false;
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  if (/Linux/i.test(ua) && !/Android/i.test(ua)) return true; // Deck: mpv binary
  // macOS: the in-process libmpv engine flags itself in Rust `setup` once it's up.
  return '__KROMA_MPV__' in globalThis;
}

// ----- Android TV ExoPlayer bridge --------------------------------------------
// The @kroma/androidtv shell hosts the app in a WebView with a native media3 /
// ExoPlayer instance rendering to a SurfaceView BEHIND it (the same "video plane
// behind the page" model as AVPlay/mpv). The Kotlin side injects this object via
// addJavascriptInterface, and pushes events by calling the global
// `__kromaExoEvent(payload)` the engine installs. Inert in a plain browser.

/** The command surface the Android shell injects as `__KROMA_ANDROID__`. */
export interface ExoShellBridge {
  /** Load a URL (replaces the current item). `master` hints HLS vs progressive. */
  load(url: string, startSec: number, master: boolean): void;
  /** JSON command: `{op: 'play'|'pause'|'seek'|'audio'|'stop', value?: number}`. */
  command(json: string): void;
}

/** The injected ExoPlayer bridge when running inside the Android TV shell, else null. */
export function getExo(): ExoShellBridge | null {
  const w = globalThis as unknown as { __KROMA_ANDROID__?: Partial<ExoShellBridge> };
  const b = w.__KROMA_ANDROID__;
  return typeof b?.load === 'function' && typeof b?.command === 'function'
    ? (b as ExoShellBridge)
    : null;
}

/** Whether to drive playback through the native ExoPlayer bridge. */
export function exoAvailable(): boolean {
  return getExo() != null;
}

/**
 * The REAL start of an anchored master: the server seeks to the keyframe
 * at-or-before the requested anchor (`-noaccurate_seek`) and reports it via the
 * `X-Hls-Start` header on the playlist. Using the REQUESTED anchor as `baseSec`
 * drifts the absolute clock by up to one GOP (seconds!), which desyncs the
 * progress bar and every absolute-time subtitle cue after a resume/seek/audio
 * switch. The web player has always corrected this; the TV engines must too.
 * Fetching the playlist here also warms the session the engine opens next.
 */
export async function resolveMasterStart(url: string, requested: number): Promise<number> {
  if (requested <= 0.5) return 0;
  try {
    const r = await fetch(url);
    const real = Number(r.headers.get('X-Hls-Start'));
    return Number.isFinite(real) ? real : requested;
  } catch {
    return requested;
  }
}
