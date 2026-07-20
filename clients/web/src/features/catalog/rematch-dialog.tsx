// "Fix the TMDB match" modal: the ranked TMDB candidates for one catalog
// element, shown as a scannable poster grid with the confidence the server's
// matcher gave each, so an operator can see why the automatic pick went wrong
// and choose the right title.
//
// Gated on `library.manage` by the caller AND by the server. Applying a choice
// re-runs the metadata stage in the background, so the modal closes on an ack
// rather than waiting for the new art (the fiche live-refreshes on the update
// event).

import { apiErrorText, type MatchCandidate } from '@kroma/core';
import { Image, useT } from '@kroma/ui';
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
  // which is what the modal opens with. Typing alone does not refetch.
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
        className="fixed inset-0 z-60 bg-[rgba(4,4,6,.66)] backdrop-blur-[3px]"
      />
      <div className="pointer-events-none fixed inset-0 z-61 flex items-center justify-center p-4">
        <section className="pointer-events-auto flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#0E0E12] shadow-[0_30px_90px_rgba(0,0,0,.6)]">
          <header className="flex items-start justify-between gap-4 border-b border-white/[0.07] px-7 py-5">
            <div className="min-w-0">
              <div className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                {t('rematch.title')}
              </div>
              <h2 className="mt-1 truncate font-display text-[22px] font-bold">{title}</h2>
              {data?.year != null ? (
                <div className="mt-0.5 text-[12.5px] text-white/45">
                  {t('rematch.parsedAs', { title: `${data.query} (${data.year})` })}
                </div>
              ) : null}
            </div>
            <form
              className="flex shrink-0 gap-2"
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
                className="w-[220px] max-w-[42vw] rounded-xl border border-white/9 bg-[#15151A] px-3.5 py-2.5 text-[14px] outline-none placeholder:text-white/30 focus:border-white/20"
              />
              <button
                type="submit"
                disabled={isPending}
                aria-label={t('rematch.search')}
                className="flex items-center rounded-xl border border-white/9 bg-[#15151A] px-3.5 hover:bg-[#1a1a20] disabled:opacity-50"
              >
                <IconSearch size={16} stroke={2.3} />
              </button>
              <button
                type="button"
                onClick={onClose}
                aria-label={t('common.close')}
                className="flex items-center rounded-xl border border-white/9 bg-[#15151A] px-2.5 text-white/60 hover:bg-[#1a1a20] hover:text-white"
              >
                <IconX size={18} stroke={2.1} />
              </button>
            </form>
          </header>

          <div className="flex-1 overflow-y-auto px-7 py-5">
            {error ? (
              <p className="mb-4 rounded-xl border border-red-500/25 bg-red-500/10 px-4 py-3 text-[13px] text-red-200">
                {error}
              </p>
            ) : null}
            {isPending ? (
              <div className="flex justify-center py-16 text-white/40">
                <IconLoader2 size={26} stroke={2.2} className="animate-spin" />
              </div>
            ) : null}
            {!isPending && data?.results.length === 0 ? (
              <p className="py-16 text-center text-[13px] text-white/40">
                {t('rematch.noResults')}
              </p>
            ) : null}
            {!isPending && (data?.results.length ?? 0) > 0 ? (
              <div className="grid grid-cols-2 gap-3.5 sm:grid-cols-3 xl:grid-cols-4">
                {data?.results.map((c) => (
                  <CandidateCard
                    key={c.tmdbId}
                    candidate={c}
                    busy={applying === c.tmdbId}
                    disabled={applying !== null}
                    onPick={() => apply(c.tmdbId)}
                  />
                ))}
              </div>
            ) : null}
          </div>

          {data?.pinned ? (
            <div className="border-t border-white/[0.07] px-7 py-4">
              <button
                type="button"
                disabled={applying !== null}
                onClick={() => apply(null)}
                className="flex w-full items-center justify-center gap-2 rounded-xl border border-white/9 px-4 py-3 text-[13px] font-bold text-white/70 hover:bg-white/4 disabled:opacity-50"
              >
                {applying === 'reset' ? (
                  <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
                ) : null}
                {t('rematch.reset')}
              </button>
            </div>
          ) : null}
        </section>
      </div>
    </>
  );
}

/** One candidate as a poster tile: art, identity, and the matcher's confidence. */
function CandidateCard({
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
      className={`group flex flex-col overflow-hidden rounded-xl border text-left transition-colors disabled:opacity-60 ${
        current
          ? 'border-accent/50 bg-accent/[0.08]'
          : 'border-white/8 bg-[#15151A] hover:border-white/20 hover:bg-[#1a1a20]'
      }`}
    >
      <div className="relative aspect-[2/3] w-full bg-white/5">
        {posterUrl ? (
          <Image src={posterUrl} fit="cover" fill />
        ) : (
          <div className="flex h-full w-full items-center justify-center text-white/15">
            <IconSearch size={26} stroke={1.6} />
          </div>
        )}
        <div className="absolute left-1.5 top-1.5">
          <Confidence score={score} />
        </div>
        {current ? (
          <span className="absolute right-1.5 top-1.5 rounded-full bg-accent px-2 py-0.5 text-[10px] font-bold uppercase tracking-[.06em] text-black">
            {t('rematch.current')}
          </span>
        ) : null}
        {busy ? (
          <div className="absolute inset-0 flex items-center justify-center bg-black/50">
            <IconLoader2 size={22} stroke={2.4} className="animate-spin text-white" />
          </div>
        ) : null}
        {!busy && current ? (
          <div className="absolute inset-0 flex items-center justify-center bg-black/0 opacity-0 transition-opacity group-hover:bg-black/40 group-hover:opacity-100">
            <IconCheck size={26} stroke={2.4} className="text-accent" />
          </div>
        ) : null}
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1 px-2.5 py-2.5">
        <div className="flex items-baseline gap-1.5">
          <span className="line-clamp-1 text-[13.5px] font-bold">{title}</span>
          {year != null ? <span className="text-[11.5px] text-white/45">{year}</span> : null}
        </div>
        {subtitle ? <div className="line-clamp-1 text-[11px] text-white/40">{subtitle}</div> : null}
        {overview ? (
          <p className="line-clamp-2 text-[11px] leading-snug text-white/35">{overview}</p>
        ) : null}
        <span className="mt-auto pt-1 text-[10px] text-white/25">#{tmdbId}</span>
      </div>
    </button>
  );
}

/** Confidence chip, coloured by how much the server trusts the match. */
function Confidence({ score }: Readonly<{ score: number }>) {
  const t = useT();
  const pct = Math.round(score * 100);
  let tone = 'bg-black/70 text-white/60';
  if (pct >= 70) tone = 'bg-emerald-500/85 text-black';
  else if (pct >= 35) tone = 'bg-amber-500/85 text-black';
  return (
    <span className={`rounded-full px-2 py-0.5 text-[10px] font-bold backdrop-blur-sm ${tone}`}>
      {t('rematch.confidence', { score: pct })}
    </span>
  );
}
