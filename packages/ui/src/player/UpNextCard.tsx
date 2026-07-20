import { Image } from '../components/Image';
import { FOCUS_RING } from './tw';

/**
 * One "À suivre" tile (§10): a 16:9 thumbnail with a duration badge, then a
 * category eyebrow, a title and an optional meta line. The same card renders in
 * the parked peek and inside the open sheet (no zoom between states); focus is
 * state-driven, so the amber ring + spring pop come from `FOCUS_RING` and hover
 * just moves focus via `onFocus` (§15).
 */
export interface UpNextItem {
  id: string;
  title: string;
  /** e.g. "S1 E4" or a year / genre line. */
  subtitle?: string;
  /** 16:9 thumbnail preferred; falls back to a subtle gradient. */
  posterUrl?: string | null;
  /** e.g. "48 min". */
  durationLabel?: string;
  /** e.g. "Épisode" or a genre. */
  categoryLabel?: string;
}

export interface UpNextCardProps {
  item: UpNextItem;
  focused: boolean;
  onActivate: () => void;
  onFocus?: () => void;
}

/**
 * Card column width inside the sheet's `flex flex-wrap gap-[26px]` (3 per row,
 * legacy-safe, no CSS grid): 3 cards + 2 gaps of 26px === 100%.
 */
export const UP_NEXT_CARD_W = 'w-[calc((100%-52px)/3)]';

/** Deterministic, subtle amber-into-charcoal placeholder when there is no still. */
function placeholderGradient(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + (id.codePointAt(i) ?? 0)) >>> 0;
  const tilt = 138 + (h % 54);
  return `linear-gradient(${tilt}deg, rgba(244,182,66,0.16) 0%, rgba(20,18,22,0.96) 64%)`;
}

export function UpNextCard({ item, focused, onActivate, onFocus }: Readonly<UpNextCardProps>) {
  return (
    <button
      type="button"
      // Marks the D-pad-focused card so the sheet can scroll it into view on key
      // nav ONLY (scrolling on pointer hover would shift the sheet under the cursor
      // and land clicks on the wrong card).
      data-focused={focused || undefined}
      onClick={onActivate}
      onMouseEnter={onFocus}
      className={`${UP_NEXT_CARD_W} block cursor-pointer rounded-[14px] border-none bg-transparent p-0 text-left outline-none transition-[transform,box-shadow] duration-180 ease-out ${focused ? FOCUS_RING : ''}`}
    >
      <div className="relative aspect-video w-full overflow-hidden rounded-[14px] bg-surface-1">
        <Image src={item.posterUrl} fit="cover" background={placeholderGradient(item.id)} fill />
        <div className="absolute inset-0 bg-[radial-gradient(120%_120%_at_50%_25%,transparent,rgba(0,0,0,0.42))]" />
        {item.durationLabel ? (
          <span className="absolute right-2.5 bottom-2.5 rounded-[7px] bg-[rgba(0,0,0,0.72)] px-[9px] py-[3px] font-sans text-[12px] font-bold tabular-nums text-white">
            {item.durationLabel}
          </span>
        ) : null}
      </div>
      {item.categoryLabel ? (
        <div className="mt-3 truncate font-sans text-[11px] font-bold uppercase tracking-[0.09em] text-accent">
          {item.categoryLabel}
        </div>
      ) : null}
      <div className="mt-1 font-sans text-[17px] font-semibold leading-tight text-text">
        {item.title}
      </div>
      {item.subtitle ? (
        <div className="mt-[3px] font-sans text-[14px] font-medium text-[rgba(244,243,240,0.5)]">
          {item.subtitle}
        </div>
      ) : null}
    </button>
  );
}
