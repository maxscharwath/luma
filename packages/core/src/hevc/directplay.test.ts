import type { AudioTrack, MediaItem } from '@kroma/client';
import { describe, expect, it } from 'vitest';
import {
  audioTrackId,
  avplayDirectPlayable,
  canDirectPlay,
  MSE_CAPS,
  masterNeedsAac,
  NATIVE_TV_CAPS,
  type PlayEnv,
  resolveAudioRelativeIndex,
  SAFARI_CAPS,
  selectEngine,
} from './directplay';

// ----- fixtures --------------------------------------------------------------

function track(p: Partial<AudioTrack> & { index: number }): AudioTrack {
  return {
    index: p.index,
    codec: p.codec ?? 'aac',
    channels: p.channels ?? null,
    language: p.language ?? null,
    title: p.title ?? null,
    default: p.default ?? false,
  };
}

function makeItem(p: {
  container?: string;
  videoCodec?: string;
  bitDepth?: number;
  audio: AudioTrack[];
}): MediaItem {
  return {
    container: p.container ?? 'mp4',
    video: { codec: p.videoCodec ?? 'h264', bitDepth: p.bitDepth ?? 8 },
    audio: p.audio[0] ?? null,
    audioTracks: p.audio,
    durationMs: 1000,
  } as unknown as MediaItem;
}

const EN_51 = (index: number) => track({ index, language: 'en', channels: 6, codec: 'eac3' });
const FR_51 = (index: number) => track({ index, language: 'fr', channels: 6, codec: 'eac3' });
const FR_COMMENTARY = (index: number) =>
  track({ index, language: 'fr', title: 'Commentary', channels: 2, codec: 'aac' });

// ----- resolveAudioRelativeIndex --------------------------------------------

describe('resolveAudioRelativeIndex', () => {
  it('resolves the commentary id even when the list is reordered', () => {
    // Served [EN 5.1, FR 5.1, FR-commentary] but delivered reordered.
    const reordered: AudioTrack[] = [FR_51(1), EN_51(0), FR_COMMENTARY(2)];
    const want = audioTrackId(FR_COMMENTARY(2));
    expect(resolveAudioRelativeIndex(reordered, want)).toBe(2);
  });

  it('disambiguates same-language tracks by channel count', () => {
    const tracks: AudioTrack[] = [
      track({ index: 0, language: 'fr', channels: 6 }),
      track({ index: 1, language: 'fr', channels: 2 }),
    ];
    // No exact index match → must score by language then channels.
    const want = { index: 5, language: 'fr', title: null, channels: 2 };
    expect(resolveAudioRelativeIndex(tracks, want)).toBe(1);
  });

  it('matches by language + channels when the wanted title is missing', () => {
    const tracks: AudioTrack[] = [
      track({ index: 3, language: 'en', channels: 6 }),
      track({ index: 4, language: 'fr', channels: 6 }),
    ];
    const want = { index: 9, language: 'en', title: null, channels: 6 };
    expect(resolveAudioRelativeIndex(tracks, want)).toBe(3);
  });

  it('returns the matching index when index and identity agree', () => {
    const tracks: AudioTrack[] = [EN_51(0), FR_51(1)];
    expect(resolveAudioRelativeIndex(tracks, audioTrackId(FR_51(1)))).toBe(1);
  });

  it('ignores a disagreeing index and resolves by identity', () => {
    const tracks: AudioTrack[] = [EN_51(0), FR_51(1)];
    // Wanted index 0 but the identity is French → must pick the FR track (index 1).
    const want = { index: 0, language: 'fr', title: null, channels: 6 };
    expect(resolveAudioRelativeIndex(tracks, want)).toBe(1);
  });

  it('falls back to the default track when nothing matches', () => {
    const tracks: AudioTrack[] = [
      track({ index: 0, language: 'en', channels: 6 }),
      track({ index: 1, language: 'de', channels: 6, default: true }),
    ];
    const want = { index: 9, language: 'ja', title: null, channels: 2 };
    expect(resolveAudioRelativeIndex(tracks, want)).toBe(1);
  });

  it('falls back to the first track when nothing matches and there is no default', () => {
    const tracks: AudioTrack[] = [
      track({ index: 0, language: 'en', channels: 6 }),
      track({ index: 1, language: 'de', channels: 6 }),
    ];
    const want = { index: 9, language: 'ja', title: null, channels: 2 };
    expect(resolveAudioRelativeIndex(tracks, want)).toBe(0);
  });

  it('returns 0 for an empty track list', () => {
    expect(
      resolveAudioRelativeIndex([], { index: 0, language: null, title: null, channels: null }),
    ).toBe(0);
  });
});

// ----- selectEngine ----------------------------------------------------------

const WEB_CHROME: PlayEnv = { platform: 'web', safari: false };
const WEB_SAFARI: PlayEnv = { platform: 'web', safari: true };
const TIZEN: PlayEnv = { platform: 'tizen', safari: false };
const WEBOS: PlayEnv = { platform: 'webos', safari: false };
const DESKTOP: PlayEnv = { platform: 'desktop', safari: false };

describe('selectEngine', () => {
  it('routes a plain h264 + aac single-audio mp4 to direct-play on Chrome', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'h264',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(selectEngine(item, WEB_CHROME)).toEqual({ kind: 'direct', aacMaster: false });
  });

  it('routes an MKV to web-mse (not direct) on Chrome', () => {
    const item = makeItem({
      container: 'mkv',
      videoCodec: 'h264',
      audio: [track({ index: 0, codec: 'aac', channels: 6, default: true })],
    });
    expect(selectEngine(item, WEB_CHROME).kind).toBe('web-mse');
  });

  it('direct-plays an HEVC mp4 on Chrome when the engine caps decode HEVC', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'hevc',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(selectEngine(item, WEB_CHROME)).toEqual({ kind: 'direct', aacMaster: false });
  });

  it('keeps HEVC off direct-play when the runtime probes NO HEVC decode', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'hevc',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    const noHevc: PlayEnv = {
      platform: 'web',
      safari: false,
      runtimeCaps: { ...MSE_CAPS, hevc: false, hevc10bit: false },
    };
    expect(selectEngine(item, noHevc)).toEqual({ kind: 'web-mse', aacMaster: false });
  });

  it('avplayDirectPlayable: HEVC+DTS MKV yes, AV1 no, unknown container no', () => {
    const mkv = makeItem({
      container: 'mkv',
      videoCodec: 'hevc',
      audio: [track({ index: 0, codec: 'dts', channels: 6, default: true })],
    });
    expect(avplayDirectPlayable(mkv)).toBe(true);
    const av1 = makeItem({
      container: 'mkv',
      videoCodec: 'av1',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(avplayDirectPlayable(av1)).toBe(false);
    const iso = makeItem({
      container: 'iso',
      videoCodec: 'h264',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(avplayDirectPlayable(iso)).toBe(false);
  });

  it('direct-plays HEVC + aac mp4 on Safari (native HEVC decode)', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'hevc',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(selectEngine(item, WEB_SAFARI)).toEqual({ kind: 'direct', aacMaster: false });
  });

  it('Safari cannot decode AV1 (no software decoder; HW is M3+ only)', () => {
    const av1 = makeItem({
      container: 'mkv',
      videoCodec: 'av1',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    // Chromium (MSE, dav1d) decodes AV1; Safari / WKWebView reports it unsupported
    // so the player warns instead of offering it and failing opaquely.
    expect(canDirectPlay(av1, MSE_CAPS).canDirectPlay).toBe(true);
    const verdict = canDirectPlay(av1, SAFARI_CAPS);
    expect(verdict.canDirectPlay).toBe(false);
    expect(verdict.messageKey).toBe('player.av1Unsupported');
  });

  it('HEVC + EAC3 (2 audio): tizen native (no aac), web-mse + aac, webos + aac', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'hevc',
      audio: [
        track({ index: 0, codec: 'eac3', language: 'en', channels: 6, default: true }),
        track({ index: 1, codec: 'eac3', language: 'fr', channels: 6 }),
      ],
    });
    expect(selectEngine(item, TIZEN)).toEqual({ kind: 'tizen-avplay', aacMaster: false });
    expect(selectEngine(item, WEB_CHROME)).toEqual({ kind: 'web-mse', aacMaster: true });
    expect(selectEngine(item, WEBOS)).toEqual({ kind: 'webos', aacMaster: true });
    // Steam Deck: native mpv decodes everything, so always its own engine, copy master.
    expect(selectEngine(item, DESKTOP)).toEqual({ kind: 'desktop-mpv', aacMaster: false });
  });

  it('legacy webOS (nativeHls): the master is handed to the native pipeline, stream-copied', () => {
    const item = makeItem({
      container: 'mkv',
      videoCodec: 'hevc',
      audio: [
        track({ index: 0, codec: 'eac3', language: 'en', channels: 6, default: true }),
        track({ index: 1, codec: 'eac3', language: 'fr', channels: 6 }),
      ],
    });
    // Old engines (Chromium < 99) can't decode HEVC via MSE; the TV pipeline plays
    // the HLS master natively and decodes surround itself - no AAC transcode.
    expect(selectEngine(item, { ...WEBOS, nativeHls: true })).toEqual({
      kind: 'webos',
      aacMaster: false,
    });
  });

  it('always routes the Steam Deck to desktop-mpv (even a plain mp4)', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'h264',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(selectEngine(item, DESKTOP)).toEqual({ kind: 'desktop-mpv', aacMaster: false });
  });

  it('always routes Android TV to android-exo (native decode, stream-copy master)', () => {
    const item = makeItem({
      container: 'mkv',
      videoCodec: 'hevc',
      audio: [track({ index: 0, codec: 'eac3', channels: 6, default: true })],
    });
    expect(selectEngine(item, { platform: 'androidtv', safari: false })).toEqual({
      kind: 'android-exo',
      aacMaster: false,
    });
  });

  it('direct-plays a plain mp4 on webOS, but always reports tizen-avplay on Tizen', () => {
    const item = makeItem({
      container: 'mp4',
      videoCodec: 'h264',
      audio: [track({ index: 0, codec: 'aac', channels: 2, default: true })],
    });
    expect(selectEngine(item, WEBOS)).toEqual({ kind: 'direct', aacMaster: false });
    expect(selectEngine(item, TIZEN)).toEqual({ kind: 'tizen-avplay', aacMaster: false });
  });
});

// ----- masterNeedsAac --------------------------------------------------------

describe('masterNeedsAac', () => {
  it('keeps an all-aac master as stream-copy on every engine', () => {
    const item = makeItem({ audio: [track({ index: 0, codec: 'aac', channels: 2 })] });
    expect(masterNeedsAac(item, MSE_CAPS)).toBe(false);
    expect(masterNeedsAac(item, SAFARI_CAPS)).toBe(false);
    expect(masterNeedsAac(item, NATIVE_TV_CAPS)).toBe(false);
  });

  it('forces AAC for EAC3 under MSE but stream-copies on Safari / native TV', () => {
    const item = makeItem({ audio: [track({ index: 0, codec: 'eac3', channels: 6 })] });
    expect(masterNeedsAac(item, MSE_CAPS)).toBe(true);
    expect(masterNeedsAac(item, SAFARI_CAPS)).toBe(false);
    expect(masterNeedsAac(item, NATIVE_TV_CAPS)).toBe(false);
  });

  it('forces AAC for DTS everywhere (never fMP4-copy-safe)', () => {
    const item = makeItem({ audio: [track({ index: 0, codec: 'dts', channels: 6 })] });
    expect(masterNeedsAac(item, MSE_CAPS)).toBe(true);
    expect(masterNeedsAac(item, SAFARI_CAPS)).toBe(true);
    expect(masterNeedsAac(item, NATIVE_TV_CAPS)).toBe(true);
  });

  it('forces AAC when any track is undecodable under the engine', () => {
    const item = makeItem({
      audio: [
        track({ index: 0, codec: 'aac', channels: 2 }),
        track({ index: 1, codec: 'eac3', channels: 6 }),
      ],
    });
    expect(masterNeedsAac(item, MSE_CAPS)).toBe(true);
    expect(masterNeedsAac(item, SAFARI_CAPS)).toBe(false);
  });

  it('forces AAC when the audio is unknown (unprobed file, no track list)', () => {
    // A stream-copy of an unknown codec risks handing MSE undecodable audio
    // (e.g. EAC3), which stalls the whole load. AAC is the safe default.
    const item = makeItem({ audio: [] });
    expect(masterNeedsAac(item, MSE_CAPS)).toBe(true);
    expect(masterNeedsAac(item, SAFARI_CAPS)).toBe(true);
    expect(masterNeedsAac(item, NATIVE_TV_CAPS)).toBe(true);
  });
});
