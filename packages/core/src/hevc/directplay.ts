// Direct-play verdict + audio support / per-track delivery planning, derived
// from the runtime {@link PlaybackCapabilities}.

import type { MessageKey, TVars } from '../i18n';
import type { AudioTrack, MediaItem } from '../types';
import { type PlaybackCapabilities, capabilities } from './capabilities';

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
 * decode AC3/EAC3/DTS/TrueHD, which yields video-but-no-sound — surfaced so the
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
 * are broadly decodable on the runtimes that would request a copy — so a chosen
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
 * multi-audio item whose video direct-plays here — regardless of audio codec.
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
 * AC3/EAC3/AAC). Otherwise — e.g. AC3/EAC3/DTS on Chrome, which can't decode them
 * via MSE — every rendition is AAC so the browser can decode (and switch) them.
 */
export function masterNeedsAac(
  item: MediaItem,
  caps: PlaybackCapabilities = capabilities(),
): boolean {
  return !audioTracksOf(item).every(
    (t) => !!t.codec && canDecodeAudioCodec(t.codec, caps) && FMP4_COPY_CODECS.has(t.codec),
  );
}

/** How to play a chosen audio track. */
export interface AudioPlan {
  /** `direct` = plain `<video src=stream>`; `hls` = the per-track remux variant. */
  mode: 'direct' | 'hls';
  /** Audio-relative index of the resolved track. */
  index: number;
  /** When `mode === 'hls'`: stream-copy the track (true) or re-encode to AAC. */
  copy: boolean;
}

/**
 * Decide how to deliver audio track `index` for an item, given the runtime.
 *
 *  - The first track, when this runtime can decode it, plays via plain
 *    direct-play (`mode: 'direct'`) — no server work, exactly today's behaviour.
 *  - Any other track (or a first track the runtime can't decode) goes through
 *    the server's per-track HLS remux. We stream-copy when the codec is both
 *    decodable here and fMP4-copy-safe (surround preserved); otherwise we
 *    re-encode to stereo AAC so there's always sound.
 *  - When the video itself can't direct-play, the remux can't help (the client
 *    couldn't decode the copied video either), so we leave it on direct-play and
 *    let the caller surface the unsupported-codec warning.
 */
export function planAudio(
  item: MediaItem,
  index: number,
  caps: PlaybackCapabilities = capabilities(),
): AudioPlan {
  const tracks = audioTracksOf(item);
  const track = tracks.find((t) => t.index === index) ?? tracks[0];
  const idx = track?.index ?? 0;
  const codec = track?.codec;
  const canDecode = canDecodeAudioCodec(codec, caps);

  if (!canDirectPlay(item, caps).canDirectPlay) return { mode: 'direct', index: 0, copy: false };
  if (idx === 0 && canDecode) return { mode: 'direct', index: 0, copy: false };

  const copy = canDecode && !!codec && FMP4_COPY_CODECS.has(codec);
  return { mode: 'hls', index: idx, copy };
}
