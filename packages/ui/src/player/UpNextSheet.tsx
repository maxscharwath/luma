import type { RemoteKey, Translate } from '@kroma/core';
import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import { useT } from '../i18n';
import type { PanelHandle } from './nav';
import { EYEBROW } from './tw';
import { UpNextCard, type UpNextItem } from './UpNextCard';
import { useGridFocus } from './useGridFocus';

export type { UpNextItem };

/** The two contextual buckets feeding the sheet (§10). For a film,
 * `nextEpisodes` is empty so only recommendations show. */
export interface UpNextData {
  nextEpisodes: UpNextItem[];
  recommendations: UpNextItem[];
}

export interface UpNextSheetProps {
  data: UpNextData;
  /** overlay === 'sheet': the sheet rises and captures the D-pad. */
  open: boolean;
  /** Chrome visible; the peek shows ONLY when revealed AND there is data. */
  revealed: boolean;
  /** Header click / ▼ from the controls: the shell opens the sheet. */
  onOpen: () => void;
  onClose: () => void;
  onPlay: (item: UpNextItem) => void;
}

/** Flat-grid column count (peek + open share it). */
const COLS = 3;
/** Pixels of the sheet that peek above the bottom edge while parked (§10). */
const PEEK_HEIGHT = 150;

interface Section {
  id: string;
  title: string;
  items: UpNextItem[];
  offset: number;
}

/** Split the data into "Épisodes suivants" then "Recommandations", tracking the
 * flat offset each section starts at so one focus index spans every card. */
function buildSections(data: UpNextData, t: Translate): Section[] {
  const sections: Section[] = [];
  if (data.nextEpisodes.length) {
    sections.push({
      id: 'episodes',
      title: t('player.nextEpisodes'),
      items: data.nextEpisodes,
      offset: 0,
    });
  }
  if (data.recommendations.length) {
    sections.push({
      id: 'recommendations',
      title: t('player.recommendations'),
      items: data.recommendations,
      offset: data.nextEpisodes.length,
    });
  }
  return sections;
}

/**
 * The YouTube-TV-style "À suivre" surface (§10): ONE sliding sheet with two
 * positions. Parked (peek) it sits low so only the header + a clipped card row
 * show, and the cards are not focusable (the shell owns ▼). Open, it rises over
 * a scrim into a scrollable grid grouped into "Épisodes suivants" then
 * "Recommandations". D-pad focus runs across the FLAT list of every card via
 * `useGridFocus` (cols=3); ▲ off the top (or Back) closes, Enter plays.
 */
export const UpNextSheet = forwardRef<PanelHandle, UpNextSheetProps>(function UpNextSheet(
  { data, open, revealed, onOpen, onClose, onPlay },
  ref,
) {
  const t = useT();
  const items = [...data.nextEpisodes, ...data.recommendations];

  const grid = useGridFocus({
    count: items.length,
    cols: COLS,
    onActivate: (i) => {
      const it = items[i];
      if (it) onPlay(it);
    },
    onExit: (edge) => {
      if (edge === 'top') onClose();
    },
    onBack: onClose,
  });

  // The sheet only owns the D-pad while open; otherwise the shell handles ▼.
  useImperativeHandle(
    ref,
    () => ({ onKey: (key: RemoteKey) => (open ? grid.onKey(key) : false) }),
    [open, grid.onKey],
  );

  // Scroll the focused card into view on D-pad nav only (grid.keyNonce bumps on
  // arrow keys, not hover), so the ring never leaves the viewport on TV while a
  // pointer hover leaves the scroll position - and the layout under it - untouched.
  const scrollRef = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: grid.keyNonce is a change-trigger (re-run on D-pad moves only), intentionally not read in the body.
  useEffect(() => {
    if (!open) return;
    scrollRef.current?.querySelector('[data-focused]')?.scrollIntoView({ block: 'nearest' });
  }, [grid.keyNonce, open]);

  if (!open && (!revealed || items.length === 0)) return null;

  const sections = buildSections(data, t);
  const grouped = sections.length > 1;

  return (
    <>
      <button
        type="button"
        aria-label={t('player.back')}
        tabIndex={-1}
        onClick={onClose}
        className={`absolute inset-0 z-43 border-none bg-[linear-gradient(180deg,rgba(0,0,0,0.1),rgba(0,0,0,0.55)_45%)] transition-opacity duration-340 ${open ? 'pointer-events-auto opacity-100' : 'pointer-events-none opacity-0'}`}
      />
      <div
        ref={scrollRef}
        className={`absolute inset-x-0 bottom-0 z-45 h-[82%] overflow-x-hidden bg-[linear-gradient(180deg,transparent,rgba(10,10,12,0.55)_12%,rgba(10,10,12,0.97)_30%)] transition-transform duration-340 ease-out ${open ? 'overflow-y-auto' : 'overflow-y-hidden'}`}
        style={{ transform: open ? 'translateY(0)' : `translateY(calc(100% - ${PEEK_HEIGHT}px))` }}
      >
        <SheetHeader
          open={open}
          title={t('player.upNextTitle')}
          onToggle={open ? onClose : onOpen}
        />
        <div className="px-14 pt-1 pb-14">
          {sections.map((sec) => (
            <section key={sec.id} className="mb-8 last:mb-0">
              {grouped ? <div className={`${EYEBROW} mb-3.5`}>{sec.title}</div> : null}
              <div className="flex flex-wrap items-start gap-[26px]">
                {sec.items.map((it, li) => {
                  const flat = sec.offset + li;
                  return (
                    <UpNextCard
                      key={it.id}
                      item={it}
                      focused={open && grid.index === flat}
                      onActivate={() => onPlay(it)}
                      onFocus={open ? grid.hover(flat) : undefined}
                    />
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      </div>
    </>
  );
});

/** The clickable header: title + a chevron that flips between the two states. */
function SheetHeader({
  open,
  title,
  onToggle,
}: Readonly<{ open: boolean; title: string; onToggle: () => void }>) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex w-full cursor-pointer items-center gap-3.5 border-none bg-transparent px-14 pt-6 pb-4 text-left outline-none"
    >
      <span className="font-display text-[22px] font-bold text-text">{title}</span>
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        strokeWidth={2.2}
        strokeLinecap="round"
        strokeLinejoin="round"
        className="stroke-accent transition-transform duration-300"
        style={{ transform: open ? 'rotate(0deg)' : 'rotate(180deg)' }}
        aria-hidden="true"
      >
        <path d="M6 9l6 6 6-6" />
      </svg>
    </button>
  );
}
