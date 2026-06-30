import { audioSupport, formatTimecode as fmtTime, type MediaItem, metaLine } from '@luma/core';
import { useT } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AvDrawer } from '#web/features/playback/AvDrawer';
import { ControlBar } from '#web/features/playback/ControlBar';
import { IconBack, IconStopped } from '#web/features/playback/icons';
import { Toast } from '#web/features/playback/PlayerToast';
import { SkipIntroButton } from '#web/features/playback/SkipIntroButton';
import { StatsOverlay } from '#web/features/playback/StatsOverlay';
import { SubtitleLayer } from '#web/features/playback/SubtitleLayer';
import { useSubtitleStyle } from '#web/features/playback/subtitleStyle';
import { UpNextOverlay } from '#web/features/playback/UpNextOverlay';
import { usePlaybackSession } from '#web/features/playback/usePlaybackSession';
import { usePlayerHotkeys } from '#web/features/playback/usePlayerHotkeys';
import { useResumeProgress } from '#web/features/playback/useResumeProgress';
import { useUpNext } from '#web/features/playback/useUpNext';
import { useVideoPlayback } from '#web/features/playback/useVideoPlayback';
import { lumaClient, type MovieView, type SubtitleView } from '#web/shared/lib/api';
import type { DownloadedSub } from '@luma/core';

/** Custom fullscreen player: scrub bar with hover preview, ±10s, volume, speed,
 * PiP, fullscreen, auto-hiding controls, full keyboard control, an audio/
 * subtitle drawer and a stats-for-nerds overlay. Mechanics live in
 * `useVideoPlayback` / `useResumeProgress`; this file is composition + chrome. */
export function Player({
  item,
  next,
  onPlayNext,
  onClose,
}: Readonly<{
  item: MovieView;
  /** Next episode for the Netflix-style "up next" autoplay (null for movies / last ep). */
  next?: MediaItem | null;
  onPlayNext?: () => void;
  onClose: () => void;
}>) {
  const t = useT();
  const pb = useVideoPlayback(item);
  const {
    videoRef,
    containerRef,
    playing,
    scrubbing,
    togglePlay,
    skip,
    setVol,
    toggleMute,
    toggleFullscreen,
  } = pb;
  // Absolute-timeline position control for resume + progress-save (the VOD master
  // and direct-play share one absolute timeline no offset).
  const position = useMemo(
    () => ({ seekTo: pb.seekTo, getPosition: pb.getPosition }),
    [pb.seekTo, pb.getPosition],
  );
  const { resumeAt, showResume, setShowResume } = useResumeProgress(videoRef, item, position);
  // Admin can remotely stop this playback → show a message and close.
  const [terminated, setTerminated] = useState<string | null>(null);
  // Heartbeat this playback to the server for the admin dashboard's live sessions.
  usePlaybackSession({
    item,
    getPosition: pb.getPosition,
    playing: pb.playing,
    mode: pb.useHls ? 'transcode' : 'direct',
    onTerminated: (message) => {
      try {
        videoRef.current?.pause();
      } catch {
        /* ignore */
      }
      setTerminated(message?.trim() || t('player.stoppedDefault'));
      window.setTimeout(onClose, 6000);
    },
  });

  const [controls, setControls] = useState(true);
  const [avOpen, setAvOpen] = useState(false);
  const [statsOpen, setStatsOpen] = useState(false);
  const [activeSub, setActiveSub] = useState<number | null>(null);
  const [subStyle, setSubStyle] = useSubtitleStyle();
  const [audioWarn, setAudioWarn] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const audio = audioSupport(item);

  // Online-downloaded subtitles, merged with the embedded tracks. Fetched on open
  // and after each download. They get high indices (1000+) so they never collide
  // with embedded ones; the URL is the cached WebVTT.
  const [downloaded, setDownloaded] = useState<DownloadedSub[]>([]);
  useEffect(() => {
    let cancelled = false;
    lumaClient()
      .downloadedSubtitles(item.id)
      .then((d) => !cancelled && setDownloaded(d))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [item.id]);
  const onDownloaded = useCallback((sub: DownloadedSub) => {
    setDownloaded((prev) => {
      if (prev.some((p) => p.id === sub.id)) return prev;
      const next = [...prev, sub];
      queueMicrotask(() => setActiveSub(1000 + next.length - 1)); // auto-enable it
      return next;
    });
  }, []);
  const allSubs = useMemo<SubtitleView[]>(() => {
    const dl: SubtitleView[] = downloaded.map((d, i) => ({
      index: 1000 + i,
      language: d.language,
      codec: 'SRT',
      url: lumaClient().resolveArt(d.url) ?? d.url,
      downloaded: true,
      label: d.label,
    }));
    return [...item.subs, ...dl];
  }, [item.subs, downloaded]);

  // Stable identity so the subtitle layer doesn't re-run effects on every timeupdate.
  const renderedSubs = useMemo(() => allSubs.filter((s) => s.url), [allSubs]);
  const subtitle =
    item.kind === 'episode' && item.showTitle
      ? `${item.showTitle} · ${item.title}`
      : metaLine(item);

  // SubtitleLayer owns rendering; we only track which subtitle is active.
  const pickSub = useCallback((index: number | null) => setActiveSub(index), []);

  // Deep-link: `#stats` opens stats-for-nerds, `#av` opens the audio/subtitle panel.
  useEffect(() => {
    const h = window.location.hash;
    if (h.includes('stats')) setStatsOpen(true);
    if (h.includes('av')) setAvOpen(true);
  }, []);

  // ----- auto-hide controls ---------------------------------------------------
  const poke = useCallback(() => {
    setControls(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => {
      if (videoRef.current && !videoRef.current.paused) setControls(false);
    }, 3000);
  }, [videoRef]);

  useEffect(() => {
    if (!playing || avOpen || statsOpen || scrubbing) setControls(true);
    else poke();
  }, [playing, avOpen, statsOpen, scrubbing, poke]);

  // ----- keyboard -------------------------------------------------------------
  usePlayerHotkeys({
    videoRef,
    togglePlay,
    skip,
    setVol,
    toggleMute,
    toggleFullscreen,
    seekTo: pb.seekTo,
    dur: pb.dur,
    pickSub,
    activeSub,
    subs: allSubs,
    avOpen,
    setAvOpen,
    setStatsOpen,
    onClose,
    poke,
  });

  // ----- skip intro -----------------------------------------------------------
  const intro = (item.markers ?? []).find((m) => m.kind === 'intro');
  const showSkipIntro =
    intro != null && pb.cur * 1000 >= intro.startMs && pb.cur * 1000 < intro.endMs;

  // ----- up next (credits-aware series autoplay) ------------------------------
  const up = useUpNext({
    item,
    next,
    onPlayNext,
    cur: pb.cur,
    dur: pb.dur,
    scrubbing: pb.scrubbing,
    terminated: terminated != null,
    videoRef,
  });

  return (
    <div
      ref={containerRef}
      onPointerMove={poke}
      className="fixed inset-0 z-60 flex items-center justify-center overflow-hidden bg-black"
      style={{ cursor: controls ? 'default' : 'none' }}
    >
      {/* `key` REMOUNTS the element when the remux is re-anchored (resume / seek)
          OR the audio language changes, so hls.js always does a clean fresh attach
          (a re-attach on a reused element is flaky and the chosen audio is muxed
          per-stream). */}
      {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
      <video
        key={`${pb.anchor}:${pb.audioIndex}`}
        ref={videoRef}
        autoPlay
        playsInline
        className="h-full w-full bg-black object-contain"
        onClick={togglePlay}
        onDoubleClick={toggleFullscreen}
      />

      {/* custom, fully-styleable subtitle renderer (fetches VTT itself) */}
      <SubtitleLayer
        videoRef={videoRef}
        rendered={renderedSubs}
        activeIndex={activeSub}
        style={subStyle}
        raised={controls}
        baseSec={pb.baseSec}
      />

      {pb.waiting && !terminated ? (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="h-14 w-14 animate-spin rounded-full border-[3px] border-white/20 border-t-accent" />
        </div>
      ) : null}

      {/* Admin stopped this stream → blocking message, then auto-close. */}
      {terminated ? (
        <div className="absolute inset-0 z-70 flex flex-col items-center justify-center gap-5 bg-black/85 px-8 text-center backdrop-blur-sm">
          <span className="text-[#E8536A]">
            <IconStopped size={52} />
          </span>
          <div className="font-display text-[24px] font-bold text-white">
            {t('player.stoppedTitle')}
          </div>
          <p className="max-w-115 text-[15px] text-white/70">{terminated}</p>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md bg-white/10 px-5 py-2.5 text-[14px] font-semibold text-white hover:bg-white/20"
          >
            {t('player.back')}
          </button>
        </div>
      ) : null}

      {/* Audio codec the browser can't decode. If the video itself direct-plays we
          switch to the HLS audio-transcode (stereo AAC) and just note it. */}
      {!audio.canPlay && audioWarn && pb.useHls ? (
        <Toast variant="info" onDismiss={() => setAudioWarn(false)}>
          {t('player.audioReencodedToast', { codec: item.audio?.codec?.toUpperCase() ?? '' })}
        </Toast>
      ) : null}

      {/* Video also undecodable here → no recovery, so warn (Safari/TV needed). */}
      {!audio.canPlay && audioWarn && !pb.useHls && audio.messageKey ? (
        <Toast variant="danger" onDismiss={() => setAudioWarn(false)}>
          ⚠ {t(audio.messageKey, audio.messageVars)}
        </Toast>
      ) : null}

      {/* Resume prompt: we auto-seek to the saved position and offer a restart. */}
      {showResume && resumeAt != null ? (
        <Toast
          variant="info"
          onDismiss={() => setShowResume(false)}
          action={
            <button
              onClick={() => {
                pb.seekTo(0); // restart from the beginning
                setShowResume(false);
              }}
              className="rounded-md bg-white/10 px-2.5 py-1 text-[12px] font-semibold text-white hover:bg-white/20"
            >
              {t('player.restart')}
            </button>
          }
        >
          ⏵ {t('player.resumeAt', { time: fmtTime(resumeAt) })}
        </Toast>
      ) : null}

      {showSkipIntro && intro ? (
        <SkipIntroButton onSkip={() => pb.seekTo(intro.endMs / 1000)} />
      ) : null}

      {up.showUpNext && next ? (
        <UpNextOverlay
          next={next}
          seconds={up.countdown}
          total={up.total}
          onPlayNow={up.advance}
          onCancel={up.cancel}
        />
      ) : null}

      {statsOpen ? (
        <StatsOverlay
          videoRef={videoRef}
          item={item}
          cur={pb.cur}
          dur={pb.dur}
          bufEnd={pb.bufEnd}
          anchor={pb.anchor}
          baseSec={pb.baseSec}
          useHls={pb.useHls}
          aac={pb.aac}
          audioTracks={pb.audioTracks}
          audioIndex={pb.audioIndex}
          hlsRef={pb.hlsRef}
          onClose={() => setStatsOpen(false)}
        />
      ) : null}

      {/* top bar */}
      <div
        className="absolute inset-x-0 top-0 flex items-center gap-4 bg-linear-to-b from-black/65 to-transparent px-8 py-6 transition-opacity duration-300"
        style={{ opacity: controls ? 1 : 0, pointerEvents: controls ? 'auto' : 'none' }}
      >
        <button
          onClick={onClose}
          className="flex h-10.5 w-10.5 items-center justify-center rounded-full border border-white/15 bg-white/10 text-white hover:bg-white/20"
          aria-label={t('player.back')}
        >
          <IconBack />
        </button>
        <div>
          <div className="font-display text-[19px] font-bold text-white">{item.title}</div>
          <div className="text-[13px] text-white/60">{subtitle}</div>
        </div>
      </div>

      {/* bottom controls */}
      <div
        className="absolute inset-x-0 bottom-0 bg-linear-to-t from-black/80 to-transparent px-8 pb-6 pt-20 transition-opacity duration-300"
        style={{ opacity: controls ? 1 : 0, pointerEvents: controls ? 'auto' : 'none' }}
      >
        <ControlBar
          pb={pb}
          statsOpen={statsOpen}
          markers={item.markers}
          onToggleStats={() => setStatsOpen((s) => !s)}
          onOpenAv={() => setAvOpen(true)}
          onPlayNext={up.canAdvance ? up.advance : undefined}
        />
      </div>

      {avOpen ? (
        <AvDrawer
          item={item}
          subs={allSubs}
          audioTracks={pb.audioTracks}
          audioIndex={pb.audioIndex}
          onPickAudio={pb.setAudio}
          activeSub={activeSub}
          onPickSub={pickSub}
          onDownloaded={onDownloaded}
          subStyle={subStyle}
          onStyleChange={setSubStyle}
          onClose={() => setAvOpen(false)}
        />
      ) : null}
    </div>
  );
}
