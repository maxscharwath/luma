// Interactive-search results inside the request drawer: one row per release
// (name, indexer, size, seeders, score or rejection), an expandable score
// breakdown, and a per-row grab button (wired to the download engine
// milestone; hidden until the release is grabbable AND grabbing exists).

import type { ScoredReleaseView } from '@luma/core';
import { useT } from '@luma/ui';
import { IconChevronDown, IconDownload, IconExternalLink } from '@tabler/icons-react';
import { useState } from 'react';
import { formatBytes } from '#web/shared/lib/adminFormat';

export function ReleaseList({
  releases,
  errors,
  canGrab,
  busy,
  onGrab,
}: Readonly<{
  releases: ScoredReleaseView[];
  errors: string[];
  canGrab: boolean;
  busy: boolean;
  onGrab: (release: ScoredReleaseView) => void;
}>) {
  const t = useT();
  const accepted = releases.filter((r) => r.score != null);
  const rejected = releases.filter((r) => r.score == null);

  return (
    <div className="flex flex-col gap-2">
      {errors.map((e) => (
        <div
          key={e}
          className="rounded-lg border border-[#F4B642]/25 bg-[#F4B642]/[0.08] px-3 py-2 text-[12px] font-semibold text-[#F4B642]"
        >
          {e}
        </div>
      ))}
      {releases.length === 0 && errors.length === 0 ? (
        <div className="rounded-lg border border-white/[0.07] bg-[#121216] px-3 py-4 text-center text-[12.5px] font-medium text-white/45">
          {t('requests.noReleases')}
        </div>
      ) : null}
      {accepted.map((r) => (
        <ReleaseRow
          key={`${r.indexerId}-${r.guid}`}
          r={r}
          canGrab={canGrab}
          busy={busy}
          onGrab={onGrab}
        />
      ))}
      {rejected.length > 0 ? (
        <div className="mt-1 text-[10px] font-bold uppercase tracking-[.12em] text-white/35">
          {t('requests.rejectedReleases', { count: String(rejected.length) })}
        </div>
      ) : null}
      {rejected.slice(0, 30).map((r) => (
        <ReleaseRow
          key={`${r.indexerId}-${r.guid}`}
          r={r}
          canGrab={canGrab}
          busy={busy}
          onGrab={onGrab}
          override
        />
      ))}
    </div>
  );
}

function ReleaseRow({
  r,
  canGrab,
  busy,
  onGrab,
  override = false,
}: Readonly<{
  r: ScoredReleaseView;
  canGrab: boolean;
  busy: boolean;
  onGrab: (release: ScoredReleaseView) => void;
  /** This row was rejected by the decision engine; grabbing it is a manual
   * override (shown with a distinct style + tooltip). */
  override?: boolean;
}>) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const rejectedRow = r.score == null;

  return (
    <div
      className={`rounded-xl border px-3 py-2.5 ${rejectedRow ? 'border-white/[0.05] bg-[#101014] opacity-70' : 'border-white/[0.07] bg-[#121216]'}`}
    >
      <div className="flex items-center gap-2.5">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <IconChevronDown
            size={13}
            stroke={2.4}
            className={`flex-[0_0_13px] text-white/40 transition-transform ${open ? '' : '-rotate-90'}`}
          />
          <span className="truncate text-[12.5px] font-semibold" title={r.title}>
            {r.title}
          </span>
        </button>
        {r.score != null ? (
          <span className="flex-[0_0_auto] rounded-full bg-accent/[0.14] px-2 py-0.5 text-[11px] font-bold tabular-nums text-accent">
            {r.score}
          </span>
        ) : null}
        {canGrab && r.grabbable ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => onGrab(r)}
            title={override ? t('requests.grabAnyway') : t('requests.grab')}
            className={`flex h-7 w-7 flex-[0_0_28px] items-center justify-center rounded-lg border disabled:opacity-50 ${
              override
                ? 'border-[#F4B642]/35 bg-[#F4B642]/10 text-[#F4B642] hover:bg-[#F4B642]/20'
                : 'border-accent/30 bg-accent/10 text-accent hover:bg-accent/20'
            }`}
          >
            <IconDownload size={13} stroke={2.3} />
          </button>
        ) : null}
      </div>

      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 pl-[23px] text-[11px] font-semibold text-white/45">
        <span className="inline-flex items-center gap-1">
          {r.indexerName}
          {r.detailsUrl ? (
            <a
              href={r.detailsUrl}
              target="_blank"
              rel="noreferrer"
              title={t('downloads.viewOnTracker')}
              className="text-white/40 hover:text-accent"
            >
              <IconExternalLink size={11} stroke={2} />
            </a>
          ) : null}
        </span>
        {r.sizeBytes != null ? <span>{formatBytes(r.sizeBytes)}</span> : null}
        {r.seeders != null ? (
          <span className="text-[#46D08D]">{t('requests.seedersN', { n: String(r.seeders) })}</span>
        ) : null}
        {r.target !== 'movie' ? (
          <span className="text-[#86A8FF]">
            {r.target === 'season'
              ? `S${String(r.season ?? 0).padStart(2, '0')} pack`
              : `S${String(r.season ?? 0).padStart(2, '0')}E${String(r.episodes?.[0] ?? 0).padStart(2, '0')}`}
          </span>
        ) : null}
        {r.rejected ? <span className="text-[#EF8091]">{r.rejected}</span> : null}
      </div>

      {open && r.breakdown.length > 0 ? (
        <div className="mt-2 flex flex-col gap-1 border-t border-white/[0.05] pl-[23px] pt-2">
          {r.breakdown.map((l) => (
            <div
              key={`${l.rule}-${l.note}`}
              className="flex items-center justify-between gap-3 text-[11px]"
            >
              <span className="min-w-0 truncate font-medium text-white/55">
                {l.rule} · {l.note}
              </span>
              <span
                className="flex-[0_0_auto] font-bold tabular-nums"
                style={{ color: l.delta >= 0 ? '#46D08D' : '#EF8091' }}
              >
                {l.delta >= 0 ? `+${l.delta}` : l.delta}
              </span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
