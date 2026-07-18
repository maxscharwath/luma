import {
  audioSupport,
  formatTimecode as fmtTime,
  type MediaItem,
  playerSubtitle,
} from '@kroma/core';
import { Player as UnifiedPlayer, useSubtitleAppearance, useT, WEB_FLAGS } from '@kroma/ui';
import { useCallback, useMemo, useState } from 'react';
import { IconBack, IconStopped } from '#web/features/playback/icons';
import { Toast } from '#web/features/playback/player-toast';
import { usePlaybackSession } from '#web/features/playback/use-playback-session';
import { useResumeProgress } from '#web/features/playback/use-resume-progress';
import { useStoryboard } from '#web/features/playback/use-storyboard';
import { useWebController } from '#web/features/playback/use-web-controller';
import { useWebUpNext } from '#web/features/playback/use-web-upnext';
import { kromaClient, type MovieView } from '#web/shared/lib/api';

/** Scrub-preview thumbnail width (px); the storyboard tile keeps 16:9. */
const PREVIEW_W = 176;

/**
 * The web player: a thin wrapper that adapts the web engine to the shared unified
 * `<Player>` (packages/ui/src/player) and layers on the web-only app concerns
 * (resume prompt, admin-stop overlay, session heartbeat). All chrome + interaction
 * live in the shared component; web feature flags enable volume / PiP / fullscreen.
 */
export function Player({
  item,
  next,
  following,
  onPlayNext,
  onPlayItem,
  onClose,
}: Readonly<{
  item: MovieView;
  next?: MediaItem | null;
  /** Upcoming episodes (sequence order) for the "up next" rail; `next` is [0]. */
  following?: MediaItem[];
  onPlayNext?: () => void;
  /** Play any up-next card (recommendation / next episode from the sheet). */
  onPlayItem?: (id: string) => void;
  onClose: () => void;
}>) {
  const t = useT();
  const wc = useWebController(item);
  const { controller, videoRef, containerRef, pb, subtitleGen } = wc;
  const [appearance, setAppearance] = useSubtitleAppearance();
  const storyboard = useStoryboard(item.id);
  const tileAt = useCallback((sec: number) => storyboard.tile(sec, PREVIEW_W), [storyboard]);
  const upNext = useWebUpNext(item, following);

  // Resume prompt (the anchor is already set to the saved position by the engine;
  // this only shows the toast + offers a restart) and the admin-stop overlay.
  const position = useMemo(
    () => ({ seekTo: pb.seekTo, getPosition: pb.getPosition }),
    [pb.seekTo, pb.getPosition],
  );
  const { resumeAt, showResume, setShowResume } = useResumeProgress(videoRef, item, position);
  const [terminated, setTerminated] = useState<string | null>(null);

  usePlaybackSession({
    item,
    getPosition: pb.getPosition,
    playing: pb.playing,
    waiting: pb.waiting,
    mode: wc.playbackMode,
    audioLabel: wc.audioLabel,
    subtitleLabel: wc.subtitleLabel,
    onTerminated: (message) => {
      try {
        videoRef.current?.pause();
      } catch {
        /* ignore */
      }
      setTerminated(message?.trim() || t('player.stoppedDefault'));
    },
  });

  // Undecodable audio with no HLS fallback: warn (Safari / TV needed).
  const audio = audioSupport(item);
  const warn =
    !audio.canPlay && !pb.useHls && audio.messageKey
      ? t(audio.messageKey, audio.messageVars)
      : null;

  const intro = useMemo(() => (item.markers ?? []).find((m) => m.kind === 'intro'), [item.markers]);
  const introActive =
    intro != null && pb.cur * 1000 >= intro.startMs && pb.cur * 1000 < intro.endMs;

  const nextTitle = next
    ? {
        title: next.episodeTitle ?? next.title,
        subtitle:
          next.season != null && next.episode != null
            ? `S${next.season} E${next.episode}`
            : undefined,
        posterUrl: kromaClient().backdropFor(next) ?? kromaClient().posterFor(next),
      }
    : null;

  const surface = (
    <video
      key={`${pb.anchor}:${pb.audioIndex}`}
      ref={videoRef}
      autoPlay
      playsInline
      crossOrigin="anonymous"
      // object-fit is set by the stage (contain normally, cover when it shrinks
      // into the settings card so the picture fills the rounded corners instead
      // of leaving black letterbox bars). borderRadius: inherit so the video
      // clips ITSELF to the card radius - a parent overflow-hidden + border-radius
      // does not clip a child <video> once the parent is transformed (WebKit /
      // Chromium), so the radius must live on the element.
      style={{
        width: '100%',
        height: '100%',
        background: '#000',
        borderRadius: 'inherit',
      }}
    >
      {/* Captions render out-of-band via the shared SubtitleRenderer; this empty
          default track satisfies the captions requirement without adding cues. */}
      <track kind="captions" />
    </video>
  );

  return (
    <UnifiedPlayer
      controller={controller}
      flags={WEB_FLAGS}
      title={item.title}
      subtitle={playerSubtitle(item)}
      warn={warn}
      markers={item.markers ?? undefined}
      tileAt={tileAt}
      appearance={appearance}
      onAppearance={setAppearance}
      subtitleGen={subtitleGen}
      upNext={upNext}
      onPlayItem={(i) => onPlayItem?.(i.id)}
      onPlayNext={onPlayNext}
      nextTitle={nextTitle}
      intro={
        intro ? { active: introActive, onSkip: () => pb.seekTo(intro.endMs / 1000) } : undefined
      }
      surface={surface}
      rootRef={containerRef}
      terminated={
        terminated ? (
          <div className="absolute inset-0 z-80 flex flex-col items-center justify-center gap-5 bg-black/85 px-8 text-center backdrop-blur-sm">
            <span className="text-[#E8536A]">
              <IconStopped size={52} />
            </span>
            <p className="max-w-115 text-[15px] text-white/80">{terminated}</p>
            <button
              type="button"
              onClick={onClose}
              className="flex items-center gap-2 rounded-md bg-accent px-6 py-2.5 text-[14px] font-semibold text-accent-ink hover:bg-accent-hover"
            >
              <IconBack />
              {t('player.back')}
            </button>
          </div>
        ) : null
      }
      onClose={onClose}
    >
      {showResume && resumeAt != null ? (
        <Toast
          variant="info"
          onDismiss={() => setShowResume(false)}
          action={
            <button
              type="button"
              onClick={() => {
                pb.seekTo(0);
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
    </UnifiedPlayer>
  );
}
