import { Image } from '../components/Image';
import { useT } from '../i18n';

/**
 * Minimal shape the credits card needs from the up-next item. Declared locally
 * (rather than importing `UpNextItem`) so this file never hard-depends on the
 * sheet module's build order; the orchestrator passes a compatible object.
 */
export interface CreditsCardItem {
  title: string;
  /** The "kind" line under the title (e.g. "S1 E4" or a genre). */
  subtitle?: string;
  posterUrl?: string | null;
}

export interface CreditsCardProps {
  item: CreditsCardItem;
  /** Remaining whole seconds before autoplay (e.g. 5..0). */
  secondsLeft: number;
  /** Countdown length the ring drains against (e.g. 5). */
  total: number;
  playFocused: boolean;
  cancelFocused: boolean;
  onPlay: () => void;
  onCancel: () => void;
}

/** The design's custom amber play triangle (fills from `currentColor`). */
function PlayGlyph() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M7 4v16l13-8z" />
    </svg>
  );
}

/**
 * Credits autoplay card (§11): a bottom-right card that surfaces during the
 * closing credits with the next episode, a draining amber countdown ring around
 * the seconds-left number, and a cancel escape. Fades in with `kpl-fade`.
 */
export function CreditsCard({
  item,
  secondsLeft,
  total,
  playFocused,
  cancelFocused,
  onPlay,
  onCancel,
}: Readonly<CreditsCardProps>) {
  const t = useT();
  const progress = total > 0 ? Math.max(0, Math.min(1, secondsLeft / total)) : 0;
  const deg = progress * 360;
  return (
    <div className="absolute right-10 bottom-14 z-38 w-[392px] rounded-[20px] border border-[rgba(255,255,255,0.12)] bg-[rgba(16,16,20,0.9)] p-5 shadow-[0_26px_64px_rgba(0,0,0,0.62)] backdrop-blur-[26px] animate-[kpl-fade_.3s_ease]">
      <div className="relative mb-4 h-[150px] overflow-hidden rounded-[14px]">
        <Image
          src={item.posterUrl}
          fit="cover"
          background="linear-gradient(135deg, rgba(244,182,66,0.16), rgba(20,18,22,0.96))"
          fill
        />
        <div className="absolute inset-0 bg-[radial-gradient(120%_120%_at_50%_25%,transparent,rgba(0,0,0,0.5))]" />
        <div
          className="absolute left-3.5 bottom-3.5 flex h-[54px] w-[54px] items-center justify-center rounded-full shadow-[0_6px_18px_rgba(0,0,0,0.5)]"
          style={{
            background: `conic-gradient(#F4B642 ${deg}deg, rgba(255,255,255,0.14) ${deg}deg)`,
          }}
        >
          <div className="flex h-[42px] w-[42px] items-center justify-center rounded-full bg-[#101014] font-sans text-[19px] font-bold tabular-nums text-white">
            {secondsLeft}
          </div>
        </div>
      </div>
      <div className="font-sans text-[11px] font-bold uppercase tracking-[0.16em] text-[rgba(244,243,240,0.5)]">
        {t('player.nextEpisode')}
      </div>
      <div className="mt-1 truncate font-display text-[19px] font-bold leading-[1.2] text-text">
        {item.title}
      </div>
      {item.subtitle ? (
        <div className="mt-[3px] font-sans text-[13px] font-semibold text-accent">
          {item.subtitle}
        </div>
      ) : null}
      <div className="mt-4 flex gap-3">
        <button
          type="button"
          onClick={onCancel}
          className={`flex flex-none cursor-pointer items-center justify-center rounded-[11px] border-none px-[18px] py-3 font-sans text-[14px] font-bold text-text outline-none transition-[background] duration-150 ease-out ${cancelFocused ? 'bg-[rgba(255,255,255,0.16)]' : 'bg-[rgba(255,255,255,0.08)]'}`}
        >
          {t('player.cancel')}
        </button>
        <button
          type="button"
          onClick={onPlay}
          className={`flex flex-1 cursor-pointer items-center justify-center gap-2 rounded-[11px] border-none py-3 font-sans text-[14px] font-bold text-accent-ink outline-none transition-[background] duration-150 ease-out ${playFocused ? 'bg-accent-hover' : 'bg-accent'}`}
        >
          <PlayGlyph />
          {t('player.playNow')}
        </button>
      </div>
    </div>
  );
}
