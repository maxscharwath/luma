import { type LumaClient, type MediaItem, posterColors } from '@luma/core';
import { useT } from '@luma/ui';
import { ForwardGlyph, PlayGlyph } from '#tv/features/playback/player/icons';
import { CTRL_OFF, CTRL_ON, FOCUS_RING, PILL } from '#tv/features/playback/player/playerStyles';
import { TvArt } from '#tv/shared/TvMedia';

interface SkipIntroProps {
  visible: boolean;
  /** Auto-focused for the whole intro window so OK skips immediately. */
  focused: boolean;
  onSkip: () => void;
}

/**
 * Floating, focusable "Skip Intro" button shown only inside the marked intro
 * segment. Rendered independently of the auto-hiding control bar, so it stays
 * up (and focused) the whole window.
 */
export function SkipIntroButton({ visible, focused, onSkip }: SkipIntroProps) {
  const t = useT();
  if (!visible) return null;
  return (
    <button
      type="button"
      data-focus=""
      onClick={onSkip}
      className={`absolute bottom-44 right-12 z-50 ${PILL} ${
        focused ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF
      }`}
    >
      <ForwardGlyph />
      {t('player.skipIntro')}
    </button>
  );
}

interface UpNextProps {
  show: boolean;
  next: MediaItem | null;
  client: LumaClient;
  /** Seconds left on the auto-advance countdown. */
  countdown: number;
  playFocused: boolean;
  cancelFocused: boolean;
  onPlay: () => void;
  onCancel: () => void;
}

/**
 * Netflix-style "Up next" card at the credits, with a live countdown and two
 * focusable actions (Play now / Cancel). The countdown itself is owned by the
 * player; this is purely the card chrome.
 */
export function UpNextCard({
  show,
  next,
  client,
  countdown,
  playFocused,
  cancelFocused,
  onPlay,
  onCancel,
}: UpNextProps) {
  const t = useT();
  if (!show || !next) return null;
  return (
    <div className="absolute bottom-44 right-12 z-50 w-[440px] rounded-2xl border border-[rgba(255,255,255,0.12)] bg-[rgba(18,18,22,0.94)] p-5 shadow-[0_24px_64px_rgba(0,0,0,0.6)]">
      <div className="mb-3 font-sans text-[14px] font-bold uppercase tracking-[0.18em] text-accent">
        {t('content.upNext')}
      </div>
      <div className="flex gap-4">
        <div className="relative aspect-video w-44 shrink-0 overflow-hidden rounded-lg bg-surface-1">
          <TvArt
            src={client.backdropFor(next) ?? client.posterFor(next)}
            colors={posterColors(next.id)}
            position="50% 30%"
          />
        </div>
        <div className="min-w-0">
          <div className="font-sans text-[15px] font-semibold text-[rgba(244,243,240,0.6)]">
            S{next.season ?? ''}E{next.episode ?? ''}
          </div>
          <div className="line-clamp-2 font-display text-[22px] font-bold text-white">
            {next.episodeTitle ?? next.title}
          </div>
          <div className="mt-2 font-sans text-[16px] font-semibold text-[rgba(244,243,240,0.7)]">
            {t('player.playingNextIn', { seconds: Math.max(0, countdown) })}
          </div>
        </div>
      </div>
      <div className="mt-4 flex gap-3">
        <button
          type="button"
          data-focus=""
          onClick={onPlay}
          className={`flex h-12 flex-1 items-center justify-center gap-2 rounded-full font-sans text-[16px] font-bold text-accent-ink transition-[transform,box-shadow,background] duration-180 ${
            playFocused ? `${FOCUS_RING} bg-accent-hover` : 'bg-accent'
          }`}
        >
          <PlayGlyph />
          {t('content.play')}
        </button>
        <button
          type="button"
          data-focus=""
          onClick={onCancel}
          className={`flex h-12 items-center justify-center rounded-full px-6 font-sans text-[16px] font-bold text-white transition-[transform,box-shadow,background] duration-180 ${
            cancelFocused ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF
          }`}
        >
          {t('common.cancel')}
        </button>
      </div>
    </div>
  );
}
