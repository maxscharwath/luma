// Direct-play verdict + audio support / per-track delivery planning, derived
// from the runtime {@link PlaybackCapabilities}.

import type { MessageKey, TVars } from '../i18n';
import type { AudioTrack, MediaItem } from '@luma/client';
import { type AudioCapabilities, capabilities, type PlaybackCapabilities } from './capabilities';

export interface DirectPlayVerdict {
  /** True when the client can directly decode this item's video codec. */
  canDirectPlay: boolean;
  /** i18n key for the human-readable reason. Translate at the call site with the
   * active locale (`t(verdict.messageKey, verdict.messageVars)`); core stays
   * language-agnostic so the same verdict renders correctly in any UI locale. */
  messageKey: MessageKey;
  /** Interpolation values for {@link messageKey}, when the message has any. */
  messageVars?: TVars;
}

/**
 * Given an item and the runtime capabilities, decide whether direct-play will
 * work. With the server's always-direct-play policy this is the gate that
 * decides whether to even offer the Play button or warn the user. The reason is
 * returned as an i18n key (see {@link DirectPlayVerdict.messageKey}).
 */
export function canDirectPlay(
  item: MediaItem,
  caps: PlaybackCapabilities = capabilities(),
): DirectPlayVerdict {
  const codec = item.video?.codec ?? 'unknown';
  const tenBit = (item.video?.bitDepth ?? 8) >= 10;

  switch (codec) {
    case 'hevc':
      if (!caps.hevc) return { canDirectPlay: false, messageKey: 'player.hevcUnsupported' };
      if (tenBit && !caps.hevc10bit)
        return { canDirectPlay: false, messageKey: 'player.hevc10Unsupported' };
      return { canDirectPlay: true, messageKey: 'player.directPlayHevc' };
    case 'h264':
      return caps.h264
        ? { canDirectPlay: true, messageKey: 'player.directPlayH264' }
        : { canDirectPlay: false, messageKey: 'player.h264Unsupported' };
    case 'av1':
      return caps.av1
        ? { canDirectPlay: true, messageKey: 'player.directPlayAv1' }
        : { canDirectPlay: false, messageKey: 'player.av1Unsupported' };
    case 'vp9':
      return caps.vp9
        ? { canDirectPlay: true, messageKey: 'player.directPlayVp9' }
        : { canDirectPlay: false, messageKey: 'player.vp9Unsupported' };
    default:
      return { canDirectPlay: true, messageKey: 'player.directPlayUnknown' };
  }
}

/** Whether this runtime can decode the item's audio track. Browsers can't
 * decode AC3/EAC3/DTS/TrueHD, which yields video-but-no-sound surfaced so the
 * player can warn the user. The warning is an i18n key (translate at the call
 * site with the active locale); `messageKey` is null when audio plays fine. */
export interface AudioSupport {
  canPlay: boolean;
  messageKey: MessageKey | null;
  messageVars?: TVars;
}

export function audioSupport(
  item: MediaItem,
  caps: PlaybackCapabilities = capabilities(),
): AudioSupport {
  const codec = item.audio?.codec;
  if (!codec) return { canPlay: true, messageKey: null };
  const ok = (caps.audio as unknown as Record<string, boolean | undefined>)[codec];
  if (ok === undefined || ok) return { canPlay: true, messageKey: null }; // unknown codec → don't block
  return {
    canPlay: false,
    messageKey: 'player.audioUnsupported',
    messageVars: { codec: codec.toUpperCase() },
  };
}

/** Audio codecs that ffmpeg can stream-copy into the fMP4 HLS variant AND that
 * are broadly decodable on the runtimes that would request a copy so a chosen
 * track in one of these is remuxed with no re-encode (surround preserved). Other
 * codecs (DTS/TrueHD/FLAC/Opus) fall back to a stereo-AAC transcode. */
export const FMP4_COPY_CODECS = new Set<string>(['aac', 'ac3', 'eac3']);

/** All audio tracks of an item, with a single-track fallback for older payloads
 * that only carry the representative `audio`. */
export function audioTracksOf(item: MediaItem): AudioTrack[] {
  if (item.audioTracks?.length) return item.audioTracks;
  return item.audio ? [{ ...item.audio, index: item.audio.index ?? 0 }] : [];
}

/** Whether this runtime can natively decode `codec`. Unknown codecs are assumed
 * decodable (we don't block on them). */
export function canDecodeAudioCodec(
  codec: string | undefined,
  caps: PlaybackCapabilities = capabilities(),
): boolean {
  if (!codec) return true;
  const ok = (caps.audio as unknown as Record<string, boolean | undefined>)[codec];
  return ok === undefined || ok;
}

/**
 * Whether this item can use the single-stream HLS *master* (all audio tracks as
 * alternate renditions) so language switches are seamless/in-place. The master
 * stream-copies the video (so the runtime must decode it) but can AAC-transcode
 * any audio rendition (see {@link masterNeedsAac}), so this holds for ANY
 * multi-audio item whose video direct-plays here regardless of audio codec.
 */
export function canSeamlessAudioSwitch(
  item: MediaItem,
  caps: PlaybackCapabilities = capabilities(),
): boolean {
  if (!canDirectPlay(item, caps).canDirectPlay) return false;
  return audioTracksOf(item).length > 1;
}

/**
 * For a master stream, whether audio must be transcoded to stereo AAC (true) or
 * can be stream-copied (false). Copy preserves surround and is used when EVERY
 * track is natively decodable AND fMP4-copy-safe here (TV/Safari with
 * AC3/EAC3/AAC). Otherwise e.g. AC3/EAC3/DTS on Chrome, which can't decode them
 * via MSE every rendition is AAC so the browser can decode (and switch) them.
 */
export function masterNeedsAac(
  item: MediaItem,
  caps: PlaybackCapabilities = capabilities(),
): boolean {
  return !audioTracksOf(item).every(
    (t) => !!t.codec && canDecodeAudioCodec(t.codec, caps) && FMP4_COPY_CODECS.has(t.codec),
  );
}

// ----- stable audio identity + reordering-robust resolver --------------------

/**
 * A STABLE identity for an audio track, decoupled from its display position.
 * Track selection must key off this (never the array order), because the server
 * can serve `item.audioTracks` in a different order than the player last saw, so
 * a positional index would silently select the wrong language after a reorder.
 */
export interface AudioTrackId {
  /** Audio-relative stream index (`-map 0:a:<index>`). */
  index: number;
  language: string | null;
  title: string | null;
  channels: number | null;
}

/** The stable {@link AudioTrackId} of an audio track. */
export function audioTrackId(t: AudioTrack): AudioTrackId {
  return {
    index: t.index,
    language: t.language ?? null,
    title: t.title ?? null,
    channels: t.channels ?? null,
  };
}

/** Whether a track agrees with `want` on every identity field (not just index). */
function sameIdentity(t: AudioTrack, want: AudioTrackId): boolean {
  return (
    (t.language ?? null) === want.language &&
    (t.title ?? null) === want.title &&
    (t.channels ?? null) === want.channels
  );
}

/** Score how well `t` matches `want`: language dominates, then channels, then
 * title. 0 = nothing in common. */
function scoreMatch(t: AudioTrack, want: AudioTrackId): number {
  let s = 0;
  const lang = t.language ?? null;
  if (want.language != null && lang != null) {
    if (lang.toLowerCase() === want.language.toLowerCase()) s += 100;
  } else if (want.language == null && lang == null) {
    s += 20; // both unknown language: weak agreement
  }
  if (want.channels != null && t.channels != null && t.channels === want.channels) s += 10;
  const tt = (t.title ?? '').trim().toLowerCase();
  const wt = (want.title ?? '').trim().toLowerCase();
  if (wt && tt && tt === wt) s += 5;
  return s;
}

/**
 * Resolve a wanted audio identity to the **audio-relative index** to select
 * (i.e. what `hls.audioTrack` / a Safari `video.audioTracks` slot expects, since
 * the master's renditions are keyed by `-map 0:a:<index>`). Robust to the track
 * list being reordered:
 *
 *  1. exact index match, if that track ALSO agrees on language+title+channels;
 *  2. else the best match by (language, then channels, then title);
 *  3. else the container's default track, else the first track, else 0.
 *
 * So tracks served as `[EN 5.1, FR 5.1, FR-commentary 2.0]` then reordered to
 * `[FR 5.1, EN 5.1, FR-commentary]` still resolve the commentary id (fr,
 * "Commentary", 2ch) to the commentary track.
 */
export function resolveAudioRelativeIndex(tracks: AudioTrack[], want: AudioTrackId): number {
  if (tracks.length === 0) return 0;

  const exact = tracks.find((t) => t.index === want.index);
  if (exact && sameIdentity(exact, want)) return exact.index;

  let best: AudioTrack | null = null;
  let bestScore = 0;
  for (const t of tracks) {
    const s = scoreMatch(t, want);
    if (s > bestScore) {
      bestScore = s;
      best = t;
    }
  }
  if (best) return best.index;

  const def = tracks.find((t) => t.default);
  return def?.index ?? tracks[0]?.index ?? 0;
}

// ----- per-engine decode profiles --------------------------------------------
// The platform `capabilities()` describes one runtime, but a single client can
// reach the media through different *engines* (a plain `<video>` element, MSE /
// hls.js, native HLS, or a TV's native decoder) that decode different codecs. We
// pick the master variant per ENGINE, not per platform, so `masterNeedsAac`
// answers correctly for the engine actually playing the stream.

const MSE_AUDIO: AudioCapabilities = {
  aac: true,
  ac3: false, // Chromium/webOS MSE cannot decode AC3/EAC3/DTS
  eac3: false,
  dts: false,
  truehd: false,
  flac: true,
  opus: true,
  mp3: true,
  vorbis: true,
};

/** Chromium MSE (hls.js on Chrome/Firefox/webOS): decodes the video codecs but
 * NOT AC3/EAC3/DTS audio, so those masters must be AAC. */
export const MSE_CAPS: PlaybackCapabilities = {
  hevc: true,
  hevc10bit: true,
  h264: true,
  av1: true,
  vp9: true,
  hdr: false,
  audio: MSE_AUDIO,
  source: 'mediaSource',
};

/** Safari native HLS: like {@link MSE_CAPS} but AC3/EAC3 decode natively, so
 * surround masters can be stream-copied. Unlike Chromium it has NO software AV1
 * decoder - AV1 in Safari / WKWebView is hardware-only (Apple Silicon M3+), so a
 * pre-M3 Mac (or any Intel Mac) cannot decode it. We report `av1: false` rather
 * than offer it and fail with an opaque "codec not supported"; the mpv engine
 * (software dav1d) is the path for AV1 on those machines. */
export const SAFARI_CAPS: PlaybackCapabilities = {
  ...MSE_CAPS,
  av1: false,
  audio: { ...MSE_AUDIO, ac3: true, eac3: true },
  source: 'videoElement',
};

const TV_AUDIO: AudioCapabilities = {
  aac: true,
  ac3: true,
  eac3: true,
  dts: true,
  truehd: true,
  flac: true,
  opus: true,
  mp3: true,
  vorbis: true,
};

/** Native TV decoder (AVPlay / native `<video>` on Tizen/webOS): decodes
 * everything, so masters can be stream-copied (surround preserved). */
export const NATIVE_TV_CAPS: PlaybackCapabilities = {
  hevc: true,
  hevc10bit: true,
  h264: true,
  av1: false,
  vp9: true,
  hdr: true,
  audio: TV_AUDIO,
  source: 'platform-tv',
};

// ----- engine selection ------------------------------------------------------

export type PlayerEngineKind =
  | 'direct'
  | 'web-mse'
  | 'tizen-avplay'
  | 'webos'
  | 'desktop-mpv'
  | 'android-exo';

export interface PlayEnv {
  platform: 'web' | 'tizen' | 'webos' | 'desktop' | 'androidtv';
  safari: boolean;
  /** Runtime-probed capabilities of a bare `<video>` element (canPlayType /
   * MediaSource), when the caller has a DOM to probe. Widens direct-play beyond
   * the static engine tables: e.g. Chrome 107+ with HEVC hardware decode
   * direct-plays an HEVC MP4 instead of paying the server remux. */
  runtimeCaps?: PlaybackCapabilities;
  /** Prefer the platform's NATIVE HLS pipeline over MSE/hls.js for the master.
   * Legacy webOS engines (Chromium < 99, pre-2024 models) cannot decode HEVC
   * through MSE, but the TV's own media pipeline plays the HLS master natively,
   * surround audio included - so the master is stream-copied, not AAC. */
  nativeHls?: boolean;
}

export interface EngineDecision {
  kind: PlayerEngineKind;
  /** When the chosen engine plays the HLS master, whether to request the AAC
   * variant (`?aac=1`) rather than stream-copy (`?aac=0`). */
  aacMaster: boolean;
}

const MP4_CONTAINERS = new Set(['mp4', 'mov', 'm4v', 'm4a', 'isom']);

/** A plain, single-audio MP4 whose video + default audio direct-play under
 * `caps`: the only shape we point a bare `<video src>` at (anything else, e.g.
 * MKV or multi-audio, goes through the HLS master so audio renditions and
 * seeking stay reliable). */
function plainCompatibleMp4(item: MediaItem, caps: PlaybackCapabilities): boolean {
  const container = (item.container ?? '').toLowerCase();
  if (!MP4_CONTAINERS.has(container)) return false;
  if (!canDirectPlay(item, caps).canDirectPlay) return false;
  const tracks = audioTracksOf(item);
  if (tracks.length !== 1) return false;
  const def = tracks.find((t) => t.default) ?? tracks[0];
  return canDecodeAudioCodec(def?.codec, caps);
}

/** Containers Samsung AVPlay demuxes natively from a plain HTTP(S) URL with
 * Range support: it plays these DIRECTLY (hardware decode, native seeking and
 * in-place audio-track selection) with the server doing nothing but sendfile. */
const AVPLAY_CONTAINERS = new Set(['mp4', 'mov', 'm4v', 'mkv', 'webm', 'ts', 'm2ts']);

/**
 * Whether Tizen's native AVPlay can play this item's ORIGINAL file directly
 * (no server remux at all): a container it demuxes + a video codec the TV
 * hardware decodes. Audio is not a gate the TV decodes all the common codecs
 * (see {@link NATIVE_TV_CAPS}) and unknown ones are attempted, with the engine
 * falling back to the HLS master on a real playback error.
 */
export function avplayDirectPlayable(item: MediaItem): boolean {
  const container = (item.container ?? '').toLowerCase();
  if (!AVPLAY_CONTAINERS.has(container)) return false;
  return canDirectPlay(item, NATIVE_TV_CAPS).canDirectPlay;
}

/**
 * Pick the playback engine (and master variant) for an item in an environment.
 * Pure so it is fully unit-tested.
 *
 *  - web: `direct` for a plain compatible single-audio MP4 whose video the
 *    runtime decodes (`env.runtimeCaps` when probed, else the static engine
 *    caps); everything else is `web-mse` with the master variant chosen by
 *    `masterNeedsAac` under Safari/MSE caps. The player keeps an error fallback
 *    from `direct` to the master, so eligibility can be optimistic.
 *  - tizen: `tizen-avplay`; whether AVPlay opens the ORIGINAL file (zero server
 *    work) or the stream-copy master is decided by {@link avplayDirectPlayable}.
 *  - webos: `direct` for a plain compatible MP4, else `webos` forcing the AAC
 *    master (its MSE path cannot decode AC3/EAC3).
 *  - steamdeck: `desktop-mpv` always. A native mpv process opens the ORIGINAL
 *    file directly (VA-API hardware decode of HEVC/etc. + all surround codecs,
 *    native seeking, in-place audio-track switching via `aid`), so the server
 *    only sends bytes. Like AVPlay the engine keeps a direct→master fallback for
 *    the rare file mpv cannot demux, so the master (when used) is stream-copy.
 *  - androidtv: `android-exo` always. The shell's media3/ExoPlayer bridge plays
 *    the ORIGINAL file directly (hardware HEVC + platform surround decode,
 *    in-place audio switching) with a direct→master fallback, so the master
 *    (when used) is stream-copy - the same shape as AVPlay/mpv.
 */
export function selectEngine(item: MediaItem, env: PlayEnv): EngineDecision {
  if (env.platform === 'desktop') {
    return { kind: 'desktop-mpv', aacMaster: false };
  }
  if (env.platform === 'androidtv') {
    return { kind: 'android-exo', aacMaster: false };
  }
  if (env.platform === 'tizen') {
    return { kind: 'tizen-avplay', aacMaster: false };
  }
  if (env.platform === 'webos') {
    if (plainCompatibleMp4(item, NATIVE_TV_CAPS)) return { kind: 'direct', aacMaster: false };
    // Legacy engines hand the master to the TV's native pipeline, which decodes
    // surround itself (stream-copy); the MSE/hls.js path cannot decode AC3/EAC3.
    return { kind: 'webos', aacMaster: !env.nativeHls };
  }
  const caps = env.safari ? SAFARI_CAPS : MSE_CAPS;
  // Direct-play eligibility follows what THIS runtime actually decodes (probed
  // via canPlayType/MediaSource) rather than a codec allowlist: modern Chromium
  // hardware-decodes HEVC in a bare `<video>` where available, and the player's
  // direct→master error fallback covers an over-optimistic probe.
  if (plainCompatibleMp4(item, env.runtimeCaps ?? caps))
    return { kind: 'direct', aacMaster: false };
  return { kind: 'web-mse', aacMaster: masterNeedsAac(item, caps) };
}
