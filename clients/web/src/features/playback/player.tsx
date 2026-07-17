import type { DownloadedSub } from '@kroma/core';
import {
  audioSupport,
  audioTrackLabel,
  formatTimecode as fmtTime,
  langName,
  type MediaItem,
  playerSubtitle,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AvDrawer } from '#web/features/playback/av-drawer';
import { ControlBar } from '#web/features/playback/control-bar';
import { IconBack, IconStopped } from '#web/features/playback/icons';
import { Toast } from '#web/features/playback/player-toast';
import { SkipIntroButton } from '#web/features/playback/skip-intro-button';
import { StatsOverlay } from '#web/features/playback/stats-overlay';
import { SubtitleLayer } from '#web/features/playback/subtitle-layer';
import { useSubtitleStyle } from '#web/features/playback/subtitle-style';
import { preferredSubIndex } from '#web/features/playback/track-prefs';
import { UpNextOverlay } from '#web/features/playback/up-next-overlay';
import { useAudioBoost } from '#web/features/playback/use-audio-boost';
import { usePlaybackSession } from '#web/features/playback/use-playback-session';
import { usePlayerHotkeys } from '#web/features/playback/use-player-hotkeys';
import { useResumeProgress } from '#web/features/playback/use-resume-progress';
import { useStoryboard } from '#web/features/playback/use-storyboard';
import { useUpNext } from '#web/features/playback/use-up-next';
import { useVideoPlayback } from '#web/features/playback/use-video-playback';
import { kromaClient, type MovieView, type SubtitleView } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

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
  // Scrub-bar hover thumbnails (YouTube-style). Generated/cached server-side.
  const storyboard = useStoryboard(item.id);
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
  // Admin can remotely stop this playback → show a simple confirm whose only
  // action is to close the player and go back (no auto-close: the user dismisses).
  const [terminated, setTerminated] = useState<string | null>(null);

  const [controls, setControls] = useState(true);
  const [avOpen, setAvOpen] = useState(false);
  const [statsOpen, setStatsOpen] = useState(false);
  const [activeSub, setActiveSub] = useState<number | null>(null);
  const [subStyle, setSubStyle] = useSubtitleStyle();
  const [audioWarn, setAudioWarn] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Auto-enable the account's preferred subtitle language once, when the session
  // has hydrated. Only matches embedded text tracks (see `preferredSubIndex`); an
  // absent preference or the "off" sentinel leaves subtitles off.
  const { user } = useAuth();
  const subPrefApplied = useRef(false);
  useEffect(() => {
    if (subPrefApplied.current || !user) return;
    subPrefApplied.current = true;
    const idx = preferredSubIndex(item.subs, user.subtitleLanguage);
    if (idx != null) setActiveSub(idx);
  }, [user, item.subs]);

  const audio = audioSupport(item);

  // Client-side volume boost (Web Audio gain + limiter). Keyed like the <video>
  // element so the graph re-attaches when a re-anchor / audio switch remounts it.
  const {
    boost,
    setBoost,
    supported: boostSupported,
  } = useAudioBoost(videoRef, `${pb.anchor}:${pb.audioIndex}`);
  // One-shot boost suggestion when the server's loudness analysis flagged the
  // mix (quiet dialogue / very wide dynamics) and no boost is active. Dismissal
  // lasts for this playback only.
  const [boostSuggestDismissed, setBoostSuggestDismissed] = useState(false);
  const audioVerdict = item.audioAnalysis?.verdict;
  const boostSuggested =
    boostSupported &&
    boost === 'off' &&
    !boostSuggestDismissed &&
    (audioVerdict === 'quietDialog' || audioVerdict === 'highDynamics');

  // Online-downloaded subtitles, merged with the embedded tracks. Fetched on open
  // and after each download. They get high indices (1000+) so they never collide
  // with embedded ones; the URL is the cached WebVTT.
  const [downloaded, setDownloaded] = useState<DownloadedSub[]>([]);
  useEffect(() => {
    let cancelled = false;
    kromaClient()
      .downloadedSubtitles(item.id)
      .then((d) => !cancelled && setDownloaded(d))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [item.id]);
  // Merge only. Auto-selecting the produced track (when a generation finishes)
  // is the drawer's job, so a background translate can't force subtitles on.
  const onDownloaded = useCallback((sub: DownloadedSub) => {
    setDownloaded((prev) => (prev.some((p) => p.id === sub.id) ? prev : [...prev, sub]));
  }, []);
  const onDeleteSub = useCallback(
    (subId: string) => {
      // Generated tracks are positional (1000 + index in `downloaded`), so the
      // splice shifts every track after the deleted one down by one. Capture the
      // deleted slot before filtering, then fix up the active selection.
      const di = downloaded.findIndex((p) => p.id === subId);
      setDownloaded((prev) => prev.filter((p) => p.id !== subId));
      if (di >= 0) {
        setActiveSub((cur) => {
          if (cur == null || cur < 1000) return cur;
          if (cur === 1000 + di) return null; // the deleted track was active
          if (cur > 1000 + di) return cur - 1; // it shifted down one slot
          return cur;
        });
      }
      void kromaClient()
        .deleteSubtitle(item.id, subId)
        .catch(() => undefined);
    },
    [item.id, downloaded],
  );
  const allSubs = useMemo<SubtitleView[]>(() => {
    const dl: SubtitleView[] = downloaded.map((d, i) => ({
      index: 1000 + i,
      language: d.language,
      codec: 'SRT',
      url: kromaClient().resolveArt(d.url) ?? d.url,
      downloaded: true,
      label: d.label,
      subId: d.id,
      provider: d.provider,
    }));
    return [...item.subs, ...dl];
  }, [item.subs, downloaded]);

  // Stable identity so the subtitle layer doesn't re-run effects on every timeupdate.
  const renderedSubs = useMemo(() => allSubs.filter((s) => s.url), [allSubs]);
  const subtitle = playerSubtitle(item);

  // SubtitleLayer owns rendering; we only track which subtitle is active.
  const pickSub = useCallback((index: number | null) => setActiveSub(index), []);

  // Heartbeat this playback to the server for the admin dashboard's live sessions.
  // We report the viewer's actual audio/subtitle choice and a `buffering` state.
  // Video is always copied; only the AAC master re-encodes audio (transcode). HLS
  // without the AAC master is a pure remux (both streams copied).
  let playbackMode: 'direct' | 'remux' | 'transcode' = 'direct';
  if (pb.useHls) playbackMode = pb.aac ? 'transcode' : 'remux';
  const activeSubTrack = activeSub == null ? null : allSubs.find((s) => s.index === activeSub);
  const subtitleLabel =
    activeSub == null
      ? t('player.subtitlesOff')
      : activeSubTrack?.label || langName(t, activeSubTrack?.language) || t('player.langUnknown');
  usePlaybackSession({
    item,
    getPosition: pb.getPosition,
    playing: pb.playing,
    waiting: pb.waiting,
    mode: playbackMode,
    audioLabel: audioTrackLabel(
      t,
      pb.audioTracks.find((a) => a.index === pb.audioIndex),
    ),
    subtitleLabel,
    onTerminated: (message) => {
      try {
        videoRef.current?.pause();
      } catch {
        /* ignore */
      }
      setTerminated(message?.trim() || t('player.stoppedDefault'));
    },
  });

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
    locked: terminated != null,
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
      {/* `crossOrigin`: stream routes are public + CORS-permissive, and a
          CORS-clean source is required for the Web Audio volume boost (a
          tainted direct-play element would output silence through the graph). */}
      {/* biome-ignore lint/a11y/useMediaCaption: subtitle tracks are attached at runtime by the player's subtitle system */}
      <video
        key={`${pb.anchor}:${pb.audioIndex}`}
        ref={videoRef}
        autoPlay
        playsInline
        crossOrigin="anonymous"
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

      {/* Admin stopped this stream → a simple confirm: one button that closes the
          player and returns to the previous page. */}
      {terminated ? (
        <div className="absolute inset-0 z-70 flex flex-col items-center justify-center gap-5 bg-black/85 px-8 text-center backdrop-blur-sm">
          <span className="text-[#E8536A]">
            <IconStopped size={52} />
          </span>
          <p className="max-w-115 text-[15px] text-white/80">{terminated}</p>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md bg-accent px-6 py-2.5 text-[14px] font-semibold text-accent-ink hover:bg-accent-hover"
          >
            {t('player.back')}
          </button>
        </div>
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
              type="button"
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

      {/* Loudness-analysis boost suggestion. Yields to the resume toast (same
          slot); it reappears once resume is dismissed. */}
      {boostSuggested && !(showResume && resumeAt != null) && !terminated ? (
        <Toast
          variant="info"
          onDismiss={() => setBoostSuggestDismissed(true)}
          action={
            <button
              type="button"
              onClick={() => {
                setBoost('med');
                setBoostSuggestDismissed(true);
              }}
              className="rounded-md bg-white/10 px-2.5 py-1 text-[12px] font-semibold text-white hover:bg-white/20"
            >
              {t('player.boostSuggestAction')}
            </button>
          }
        >
          ♪{' '}
          {t(
            audioVerdict === 'quietDialog'
              ? 'player.boostSuggestQuietDialog'
              : 'player.boostSuggestHighDynamics',
          )}
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
        className="absolute inset-x-0 top-0 flex items-center gap-4 bg-linear-to-b from-black/65 to-transparent px-[max(1rem,env(safe-area-inset-left),env(safe-area-inset-right))] pb-6 pt-[max(1.5rem,env(safe-area-inset-top))] transition-opacity duration-300 sm:px-[max(2rem,env(safe-area-inset-left),env(safe-area-inset-right))]"
        style={{ opacity: controls ? 1 : 0, pointerEvents: controls ? 'auto' : 'none' }}
      >
        <button
          type="button"
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
        className="absolute inset-x-0 bottom-0 bg-linear-to-t from-black/80 to-transparent px-[max(1rem,env(safe-area-inset-left),env(safe-area-inset-right))] pb-[max(1.5rem,env(safe-area-inset-bottom))] pt-20 transition-opacity duration-300 sm:px-[max(2rem,env(safe-area-inset-left),env(safe-area-inset-right))]"
        style={{ opacity: controls ? 1 : 0, pointerEvents: controls ? 'auto' : 'none' }}
      >
        <ControlBar
          pb={pb}
          storyboard={storyboard}
          statsOpen={statsOpen}
          markers={item.markers ?? undefined}
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
          boost={boost}
          onBoost={setBoost}
          boostSupported={boostSupported}
          activeSub={activeSub}
          onPickSub={pickSub}
          onDownloaded={onDownloaded}
          onDeleteSub={onDeleteSub}
          subStyle={subStyle}
          onStyleChange={setSubStyle}
          onClose={() => setAvOpen(false)}
        />
      ) : null}
    </div>
  );
}
