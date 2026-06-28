import { audioSupport, formatTimecode as fmtTime, metaLine } from '@luma/core';
import { useT } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AvDrawer } from '#web/components/player/AvDrawer';
import { ControlBar } from '#web/components/player/ControlBar';
import { IconBack, IconStopped } from '#web/components/player/icons';
import { StatsOverlay } from '#web/components/player/StatsOverlay';
import { SubtitleLayer } from '#web/components/player/SubtitleLayer';
import { useSubtitleStyle } from '#web/components/player/subtitleStyle';
import { usePlaybackSession } from '#web/components/player/usePlaybackSession';
import { useResumeProgress } from '#web/components/player/useResumeProgress';
import { useVideoPlayback } from '#web/components/player/useVideoPlayback';
import type { MovieView } from '#web/lib/api';

/** Custom fullscreen player: scrub bar with hover preview, ±10s, volume, speed,
 * PiP, fullscreen, auto-hiding controls, full keyboard control, an audio/
 * subtitle drawer and a stats-for-nerds overlay. Mechanics live in
 * `useVideoPlayback` / `useResumeProgress`; this file is composition + chrome. */
export function Player({ item, onClose }: Readonly<{ item: MovieView; onClose: () => void }>) {
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
  // Offset-aware position control so resume + progress-save use the REAL time
  // (the seamless stream's own timeline is relative to the server -ss offset).
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
  // Stable identity so the subtitle layer doesn't re-run effects on every timeupdate.
  const renderedSubs = useMemo(() => item.subs.filter((s) => s.url), [item.subs]);
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
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      switch (e.key) {
        case ' ':
        case 'k':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
          skip(-5);
          break;
        case 'ArrowRight':
          skip(5);
          break;
        case 'j':
          skip(-10);
          break;
        case 'l':
          skip(10);
          break;
        case 'ArrowUp':
          e.preventDefault();
          setVol((videoRef.current?.volume ?? 1) + 0.05);
          break;
        case 'ArrowDown':
          e.preventDefault();
          setVol((videoRef.current?.volume ?? 1) - 0.05);
          break;
        case 'm':
          toggleMute();
          break;
        case 'f':
          toggleFullscreen();
          break;
        case 'i':
          setStatsOpen((s) => !s);
          break;
        case 'c':
          pickSub(activeSub == null ? (item.subs.find((s) => s.url)?.index ?? null) : null);
          break;
        case 'Escape':
          if (avOpen) setAvOpen(false);
          else if (document.fullscreenElement) void document.exitFullscreen();
          else onClose();
          break;
        default:
          // Number keys → jump to N/10 of the movie (offset-aware via seekTo).
          if (/^[0-9]$/.test(e.key) && pb.dur) {
            pb.seekTo((Number(e.key) / 10) * pb.dur);
          }
      }
      poke();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    videoRef,
    togglePlay,
    skip,
    setVol,
    toggleMute,
    toggleFullscreen,
    pickSub,
    activeSub,
    item.subs,
    avOpen,
    onClose,
    poke,
    pb.seekTo,
    pb.dur,
  ]);

  return (
    <div
      ref={containerRef}
      onPointerMove={poke}
      className="fixed inset-0 z-60 flex items-center justify-center overflow-hidden bg-black"
      style={{ cursor: controls ? 'default' : 'none' }}
    >
      {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
      <video
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
        offset={pb.baseSec}
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
                if (videoRef.current) videoRef.current.currentTime = 0;
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

      {statsOpen ? (
        <StatsOverlay
          videoRef={videoRef}
          item={item}
          cur={pb.cur}
          dur={pb.dur}
          bufEnd={pb.bufEnd}
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
          onToggleStats={() => setStatsOpen((s) => !s)}
          onOpenAv={() => setAvOpen(true)}
        />
      </div>

      {avOpen ? (
        <AvDrawer
          item={item}
          audioTracks={pb.audioTracks}
          audioIndex={pb.audioIndex}
          onPickAudio={pb.setAudio}
          activeSub={activeSub}
          onPickSub={pickSub}
          subStyle={subStyle}
          onStyleChange={setSubStyle}
          onClose={() => setAvOpen(false)}
        />
      ) : null}
    </div>
  );
}

/** Centered top toast for transient player notices (audio re-encode, resume, errors). */
function Toast({
  variant,
  onDismiss,
  action,
  children,
}: Readonly<{
  variant: 'info' | 'danger';
  onDismiss: () => void;
  action?: React.ReactNode;
  children: React.ReactNode;
}>) {
  const t = useT();
  const border = variant === 'danger' ? 'border-danger/40' : 'border-white/15';
  return (
    <div
      className={`absolute left-1/2 top-6 z-40 flex max-w-160 -translate-x-1/2 items-center gap-3 rounded-xl border ${border} bg-black/80 px-4 py-3 backdrop-blur-md`}
    >
      <span className="text-[13px] text-white/90">{children}</span>
      {action}
      <button
        onClick={onDismiss}
        className="text-white/50 hover:text-white"
        aria-label={t('player.dismiss')}
      >
        ✕
      </button>
    </div>
  );
}
