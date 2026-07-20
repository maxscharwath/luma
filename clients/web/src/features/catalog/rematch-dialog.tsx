// "Fix the TMDB match" drawer: the ranked TMDB candidates for one catalog
// element, with the confidence the server's matcher gave each, so an operator
// can see why the automatic pick went wrong and choose the right title.
//
// Gated on `library.manage` by the caller AND by the server. Applying a choice
// re-runs the metadata stage in the background, so the drawer closes on an ack
// rather than waiting for the new art.

import { apiErrorText, type MatchCandidate } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconCheck, IconLoader2, IconSearch, IconX } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';

type Kind = 'movie' | 'show';

export function RematchDialog({
  kind,
  id,
  title,
  onClose,
  onApplied,
}: Readonly<{
  kind: Kind;
  id: string;
  /** The catalog title, shown as the thing being corrected. */
  title: string;
  onClose: () => void;
  /** Fired once the correction is queued, so the page can flash a confirmation. */
  onApplied: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  // `submitted` drives the request; `undefined` means "search the parsed title",
  // which is what the drawer opens with. Typing alone does not refetch.
  const [submitted, setSubmitted] = useState<string | undefined>(undefined);
  // `null` = untouched, so the box can show whatever the server searched for.
  const [typed, setTyped] = useState<string | null>(null);
  const [applying, setApplying] = useState<number | 'reset' | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);

  const {
    data,
    isPending,
    error: loadError,
  } = useQuery({
    queryKey: ['rematch', kind, id, submitted ?? ''] as const,
    queryFn: () => client.matchCandidates(kind, id, submitted),
    // TMDB search is a live third-party call; a stale candidate list is useless.
    staleTime: 0,
  });

  const query = typed ?? data?.query ?? '';

  const apply = (tmdbId: number | null) => {
    setApplying(tmdbId ?? 'reset');
    setApplyError(null);
    client
      .setMatch(kind, id, tmdbId)
      .then(() => {
        onApplied();
        onClose();
      })
      .catch((e) => setApplyError(apiErrorText(e, t('rematch.applyFailed'))))
      .finally(() => setApplying(null));
  };

  const error = applyError ?? (loadError ? apiErrorText(loadError, t('rematch.loadFailed')) : null);

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={onClose}
        className="fixed inset-0 z-[60] bg-[rgba(4,4,6,.6)] backdrop-blur-[2px]"
      />
      <aside className="fixed right-0 top-0 z-[61] flex h-screen w-[460px] max-w-[94vw] flex-col border-l border-white/[0.09] bg-[#0E0E12] shadow-[-20px_0_60px_rgba(0,0,0,.6)]">
        <div className="flex items-start justify-between gap-4 border-b border-white/[0.07] px-6 py-5">
          <div className="min-w-0">
            <div className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
              {t('rematch.title')}
            </div>
            <h2 className="mt-1 truncate font-display text-[19px] font-bold">{title}</h2>
            {data?.year != null ? (
              <div className="mt-0.5 text-[12px] text-white/45">
                {t('rematch.parsedAs', { title: `${data.query} (${data.year})` })}
              </div>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('common.close')}
            className="shrink-0 text-white/60 hover:text-white"
          >
            <IconX size={20} stroke={2.1} />
          </button>
        </div>

        <form
          className="flex gap-2 border-b border-white/[0.07] px-6 py-4"
          onSubmit={(e) => {
            e.preventDefault();
            setSubmitted(query.trim() || undefined);
          }}
        >
          <input
            value={query}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={t('rematch.searchPlaceholder')}
            aria-label={t('rematch.searchPlaceholder')}
            className="min-w-0 flex-1 rounded-xl border border-white/[0.09] bg-[#15151A] px-3.5 py-2.5 text-[14px] outline-none placeholder:text-white/30 focus:border-white/20"
          />
          <button
            type="submit"
            disabled={isPending}
            aria-label={t('rematch.search')}
            className="flex items-center gap-1.5 rounded-xl border border-white/[0.09] bg-[#15151A] px-3.5 text-[13px] font-bold hover:bg-[#1a1a20] disabled:opacity-50"
          >
            <IconSearch size={15} stroke={2.3} />
          </button>
        </form>

        <div className="flex-1 overflow-y-auto px-6 py-4">
          {error ? (
            <p className="mb-3 rounded-xl border border-red-500/25 bg-red-500/10 px-4 py-3 text-[13px] text-red-200">
              {error}
            </p>
          ) : null}
          {isPending ? (
            <div className="flex justify-center py-10 text-white/40">
              <IconLoader2 size={22} stroke={2.2} className="animate-spin" />
            </div>
          ) : null}
          {!isPending && data?.results.length === 0 ? (
            <p className="py-10 text-center text-[13px] text-white/40">{t('rematch.noResults')}</p>
          ) : null}
          {!isPending
            ? data?.results.map((c) => (
                <CandidateRow
                  key={c.tmdbId}
                  candidate={c}
                  busy={applying === c.tmdbId}
                  disabled={applying !== null}
                  onPick={() => apply(c.tmdbId)}
                />
              ))
            : null}
        </div>

        {data?.pinned ? (
          <div className="border-t border-white/[0.07] px-6 py-4">
            <button
              type="button"
              disabled={applying !== null}
              onClick={() => apply(null)}
              className="flex w-full items-center justify-center gap-2 rounded-xl border border-white/[0.09] px-4 py-3 text-[13px] font-bold text-white/70 hover:bg-white/[0.04] disabled:opacity-50"
            >
              {applying === 'reset' ? (
                <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
              ) : null}
              {t('rematch.reset')}
            </button>
          </div>
        ) : null}
      </aside>
    </>
  );
}

/** One candidate: poster, identity, and the matcher's confidence. */
function CandidateRow({
  candidate,
  busy,
  disabled,
  onPick,
}: Readonly<{
  candidate: MatchCandidate;
  busy: boolean;
  disabled: boolean;
  onPick: () => void;
}>) {
  const t = useT();
  const { title, originalTitle, year, posterUrl, overview, score, current, tmdbId } = candidate;
  // The original title only earns its line when it differs from the localized one.
  const subtitle = originalTitle && originalTitle !== title ? originalTitle : null;
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onPick}
      className={`mb-2 flex w-full gap-3 rounded-xl border px-3 py-3 text-left transition-colors disabled:opacity-60 ${
        current
          ? 'border-accent/40 bg-accent/[0.07]'
          : 'border-white/[0.08] bg-[#15151A] hover:bg-[#1a1a20]'
      }`}
    >
      {posterUrl ? (
        <img
          src={posterUrl}
          alt=""
          loading="lazy"
          className="h-[84px] w-[56px] shrink-0 rounded-md object-cover"
        />
      ) : (
        <div className="h-[84px] w-[56px] shrink-0 rounded-md bg-white/[0.05]" />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-[14px] font-bold">{title}</span>
          {year != null ? <span className="text-[12px] text-white/45">{year}</span> : null}
        </div>
        {subtitle ? <div className="truncate text-[12px] text-white/40">{subtitle}</div> : null}
        <div className="mt-1 flex flex-wrap items-center gap-1.5">
          <Confidence score={score} />
          {current ? (
            <span className="rounded-full bg-accent/20 px-2 py-0.5 text-[10px] font-bold uppercase tracking-[.08em] text-accent">
              {t('rematch.current')}
            </span>
          ) : null}
          <span className="text-[10px] text-white/25">#{tmdbId}</span>
        </div>
        {overview ? (
          <p className="mt-1 line-clamp-2 text-[11.5px] leading-snug text-white/35">{overview}</p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center text-white/40">
        {busy ? <IconLoader2 size={16} stroke={2.4} className="animate-spin" /> : null}
        {!busy && current ? <IconCheck size={16} stroke={2.4} className="text-accent" /> : null}
      </div>
    </button>
  );
}

/** Confidence chip, coloured by how much the server trusts the match. */
function Confidence({ score }: Readonly<{ score: number }>) {
  const t = useT();
  const pct = Math.round(score * 100);
  let tone = 'bg-white/[0.07] text-white/45';
  if (pct >= 70) tone = 'bg-emerald-500/15 text-emerald-300';
  else if (pct >= 35) tone = 'bg-amber-500/15 text-amber-300';
  return (
    <span className={`rounded-full px-2 py-0.5 text-[10px] font-bold ${tone}`}>
      {t('rematch.confidence', { score: pct })}
    </span>
  );
}
