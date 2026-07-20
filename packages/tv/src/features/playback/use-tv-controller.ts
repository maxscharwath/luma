import { type KromaClient, type MediaItem, qualityBadgeForVideo } from '@kroma/core';
import { type PlayerController, useAudioFilter, useT } from '@kroma/ui';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { availableEngines, ENGINE_LABEL_KEY, type EnginePref } from '#tv/app/enginePref';
import { type Playback, useDirectPlayback } from '#tv/features/playback/player/useDirectPlayback';
import { buildTvStats } from '#tv/features/playback/tv-stats';
import { type TvSubtitles, useTvSubtitles } from '#tv/features/playback/use-tv-subtitles';

export interface TvController {
  controller: PlayerController;
  /** Underlying engine hook (surface refs, resume, warn live in the wrapper). */
  pb: Playback;
  subtitleGen: TvSubtitles['subtitleGen'];
}

/**
 * Adapts the TV engine (`useDirectPlayback`, driving AVPlay / mpv / ExoPlayer /
 * hls.js) + subtitle state into the shared {@link PlayerController}. Volume, PiP
 * and fullscreen are TV-off (handled by the set / already fullscreen) and
 * playback speed / loop are not exposed by the native engines - surfaced
 * honestly as no-ops so the shared chrome hides or disables them. Audio filters
 * work on EVERY surface: Web Audio on the in-page <video>, in-engine DSP on the
 * native planes.
 */
export function useTvController(client: KromaClient, item: MediaItem): TvController {
  const t = useT();
  const pb = useDirectPlayback(client, item);
  const subs = useTvSubtitles(client, item);

  // Audio normalizer (§7), one persisted mode across every engine. The Web Audio
  // compressor taps the in-page <video> (HTML engine: legacy webOS, the macOS
  // desktop webview, a desktop browser); the native planes implement the same
  // modes in-engine (mpv `af`, ExoPlayer DynamicsProcessing, AVPlay via the
  // server's filtered remux), driven through the engine port below.
  const filter = useAudioFilter(pb.videoRef, `${item.id}:${pb.surface}`);
  const { setAudioFilter: pushEngineFilter, surface } = pb;
  useEffect(() => {
    if (surface !== 'video') pushEngineFilter(filter.mode);
  }, [surface, pushEngineFilter, filter.mode]);

  const scrubPreview = useCallback(
    (abs: number | null) => {
      if (abs != null) pb.seekScrub(abs);
    },
    [pb],
  );

  const qualities = useMemo(() => {
    const badge = qualityBadgeForVideo(item.video);
    const badgeSuffix = badge ? ` · ${badge}` : '';
    return [{ id: 'auto', label: `${t('player.qualityAuto')}${badgeSuffix}` }];
  }, [item.video, t]);

  // Engine picker (Settings): the engines this platform actually offers (Tizen ->
  // AVPlay/remux, webOS -> direct/remux, desktop -> direct/remux/mpv, ...). A
  // single-option list hides the row (nothing to switch).
  const engines = useMemo(() => {
    const list = availableEngines();
    return list.length > 1 ? list.map((id) => ({ id, label: t(ENGINE_LABEL_KEY[id]) })) : [];
  }, [t]);

  const statsRef = useRef<() => ReturnType<typeof buildTvStats>>(() => ({}));
  statsRef.current = () =>
    buildTvStats({
      item,
      cur: pb.cur,
      dur: pb.dur,
      bufEnd: pb.bufEnd,
      audioTracks: pb.audioTracks,
      audioIndex: pb.audioIndex,
      video: pb.videoRef.current,
      mode: pb.surface,
      t,
    });
  const getStats = useCallback(() => statsRef.current(), []);

  const controller: PlayerController = {
    cur: pb.cur,
    dur: pb.dur,
    bufEnd: pb.bufEnd,
    seekPreview: pb.seekPreview,
    playing: pb.playing,
    waiting: pb.waiting,
    ready: pb.ready,
    error: null,
    endedNonce: pb.endedNonce,
    surface: pb.surface,
    togglePlay: pb.togglePlay,
    seekTo: pb.seekTo,
    skip: pb.seek,
    scrubPreview,
    scrubCommit: pb.seekScrubCommit,
    volume: 1,
    muted: false,
    setVolume: () => undefined,
    toggleMute: () => undefined,
    rate: 1,
    setRate: () => undefined,
    loop: false,
    setLoop: () => undefined,
    audioTracks: pb.audioTracks,
    audioIndex: pb.audioIndex,
    setAudio: pb.setAudio,
    subtitles: subs.subtitles,
    subtitleIndex: subs.activeIndex,
    setSubtitle: subs.setActive,
    qualities,
    qualityId: 'auto',
    setQuality: () => undefined,
    engines,
    engineId: pb.enginePref,
    setEngine: (id: string) => pb.setEngine(id as EnginePref),
    audioFilter: filter.mode,
    setAudioFilter: filter.setMode,
    // In-page <video> needs Web Audio; a native plane answers for its own DSP
    // (ExoPlayer has none before API 28 or on passthrough, and AVPlay loses it
    // when the server's filtered remux fails), so the row hides instead of
    // showing a mode that is doing nothing.
    audioFilterSupported: pb.surface === 'video' ? filter.supported : pb.audioFilterSupported,
    pipActive: false,
    togglePip: () => undefined,
    fullscreen: false,
    toggleFullscreen: () => undefined,
    setPlaneRect: pb.setPlaneRect,
    getStats,
  };

  return { controller, pb, subtitleGen: subs.subtitleGen };
}
