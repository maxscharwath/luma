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

/** Map an audio-relative rendition index to AVPlay's absolute stream index.
 *
 * `getTotalTrackInfo()` lists the AUDIO streams in container order, so the
 * rendition position picks the matching stream; that stream's `.index` is the
 * ABSOLUTE index `setSelectTrack` expects (audio/video/text are interleaved, so
 * the absolute index need not equal the audio-relative one). This indirection is
 * what keeps a reordered track list selecting the right language on the native
 * path. Returns `null` when the rendition is out of range. */
export function audioAbsoluteIndex(audioStreams: AvplayTrack[], rendition: number): number | null {
  return audioStreams[rendition]?.index ?? null;
}

/** The native AVPlay API when running on a Tizen device, else `null`. */
export function getAvplay(): AvplayApi | null {
  const w = globalThis as unknown as AvplayGlobal;
  return w.webapis?.avplay ?? null;
}

/** Whether to drive playback through native AVPlay (Tizen only). */
export function avplayAvailable(): boolean {
  return getAvplay() != null;
}
