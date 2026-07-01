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
  readonly kind: 'video' | 'avplay';
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
