// Season selection sheet for a TV request: per-season checkboxes (already-owned
// or requested seasons disabled with their chip), a "toutes les saisons"
// master, and the request action.

import { useT } from '@luma/ui';
import { IconCheck, IconLoader2, IconX } from '@tabler/icons-react';
import { useState } from 'react';
import { RequestStatusChip } from '#web/features/requests/request-status-chip';
import type { TitleSeason } from '#web/shared/lib/titleView';

export function SeasonPicker({
  seasons,
  title,
  busy,
  initial,
  onClose,
  onRequest,
}: Readonly<{
  seasons: TitleSeason[];
  title: string;
  busy: boolean;
  /** Seasons ticked when the sheet opens; omit to preselect every open season
   * (e.g. clicking a single season card opens the sheet with just that one). */
  initial?: number[];
  onClose: () => void;
  /** `null` = all seasons. */
  onRequest: (seasons: number[] | null) => void;
}>) {
  const t = useT();
  // Seasons still requestable (not already fully available or requested).
  const open = seasons.filter((s) => !s.available && !s.requested);
  const [selected, setSelected] = useState<Set<number>>(
    () => new Set(initial ?? open.map((s) => s.number)),
  );

  const toggle = (season: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(season)) next.delete(season);
      else next.add(season);
      return next;
    });
  };
  const allOpen = open.length > 0 && open.every((s) => selected.has(s.number));
  const toggleAll = () => setSelected(allOpen ? new Set() : new Set(open.map((s) => s.number)));

  const submit = () => {
    // Whole show when every season is picked; otherwise the subset.
    const all = seasons.length === selected.size && open.length === seasons.length;
    onRequest(all ? null : Array.from(selected).sort((a, b) => a - b));
  };

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={onClose}
        className="fixed inset-0 z-[60] bg-[rgba(4,4,6,.6)] backdrop-blur-[2px]"
      />
      <aside className="fixed right-0 top-0 z-[61] flex h-screen w-[420px] max-w-[92vw] flex-col border-l border-white/[0.09] bg-[#0E0E12] shadow-[-20px_0_60px_rgba(0,0,0,.6)]">
        <div className="flex items-center justify-between border-b border-white/[0.07] px-6 py-5">
          <div>
            <div className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
              {t('discover.requestSeasons')}
            </div>
            <h2 className="mt-1 font-display text-[19px] font-bold">{title}</h2>
          </div>
          <button type="button" onClick={onClose} className="text-white/60 hover:text-white">
            <IconX size={20} stroke={2.1} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-4">
          {open.length > 1 ? (
            <button
              type="button"
              onClick={toggleAll}
              className="mb-2 flex w-full items-center gap-3 rounded-xl border border-white/[0.08] bg-[#15151A] px-4 py-3 text-left"
            >
              <Box on={allOpen} />
              <span className="text-[14px] font-bold">{t('discover.allSeasons')}</span>
            </button>
          ) : null}
          {seasons.map((s) => (
            <SeasonRow
              key={s.number}
              s={s}
              checked={selected.has(s.number)}
              onToggle={() => toggle(s.number)}
            />
          ))}
        </div>

        <div className="border-t border-white/[0.07] px-6 py-4.5">
          <button
            type="button"
            disabled={busy || selected.size === 0}
            onClick={submit}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3.5 text-[14px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-50"
          >
            {busy ? <IconLoader2 size={16} stroke={2.4} className="animate-spin" /> : null}
            {t('discover.requestN', { n: String(selected.size) })}
          </button>
        </div>
      </aside>
    </>
  );
}

function SeasonRow({
  s,
  checked,
  onToggle,
}: Readonly<{ s: TitleSeason; checked: boolean; onToggle: () => void }>) {
  const t = useT();
  const locked = s.available || s.requested;
  return (
    <button
      type="button"
      disabled={locked}
      onClick={onToggle}
      className={`mb-2 flex w-full items-center gap-3 rounded-xl border px-4 py-3 text-left ${locked ? 'border-white/[0.05] bg-[#101014] opacity-70' : 'border-white/[0.08] bg-[#15151A] hover:bg-[#1a1a20]'}`}
    >
      {locked ? (
        <span className="flex-[0_0_auto]">
          <RequestStatusChip status={s.available ? 'available' : 'pending'} size="card" />
        </span>
      ) : (
        <Box on={checked} />
      )}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-bold">
          {s.name ?? t('discover.seasonN', { n: String(s.number) })}
        </span>
        <span className="block text-[12px] font-medium text-white/45">
          {t('discover.episodesN', { n: String(s.episodeCount) })}
        </span>
      </span>
    </button>
  );
}

function Box({ on }: Readonly<{ on: boolean }>) {
  return (
    <span
      className={`flex h-5 w-5 flex-[0_0_20px] items-center justify-center rounded-md border ${on ? 'border-accent bg-accent text-accent-ink' : 'border-white/25'}`}
    >
      {on ? <IconCheck size={13} stroke={3} /> : null}
    </span>
  );
}
