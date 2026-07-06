// The discovery-detail hero: a cinematic key-art header for a TMDB title the
// library may not have yet. Mirrors the catalog `DetailHero` look (backdrop,
// big title, crew line) but its CTA is Request / status chip / View-in-library
// instead of Play, since there's nothing to stream.

import {
  type CrewMember,
  type DiscoverDetail,
  formatRuntime,
  posterColors,
  type RequestStatus,
  sizedImageUrl,
} from '@luma/core';
import { useT } from '@luma/ui';
import { IconChevronLeft, IconLoader2, IconPlus } from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { HeroBackdrop } from '#web/features/catalog/HeroBackdrop';
import { RequestStatusChip } from '#web/features/requests/RequestStatusChip';

/** Directors + creators for the "Réalisation" line (TMDB writers are dropped
 * here to keep it terse, matching the catalog fiche). */
function directorsOf(crew: CrewMember[]): string[] {
  return crew.filter((c) => c.job === 'Director' || c.job === 'Creator').map((c) => c.name);
}

/** "2024 · 2h08" year + runtime (runtime is movie-only on TMDB). */
function metaLine(detail: DiscoverDetail): string {
  const parts: string[] = [];
  if (detail.year) parts.push(String(detail.year));
  const rt = formatRuntime((detail.runtimeMin ?? 0) * 60000);
  if (rt) parts.push(rt);
  return parts.join(' · ');
}

export function DiscoverHero({
  detail,
  busy,
  error,
  onBack,
  onRequest,
  onViewLibrary,
}: Readonly<{
  detail: DiscoverDetail;
  busy: boolean;
  error: string | null;
  onBack: () => void;
  onRequest: () => void;
  onViewLibrary: () => void;
}>) {
  const t = useT();
  const navigate = useNavigate();
  const [c1, c2] = posterColors(String(detail.tmdbId));
  const poster = sizedImageUrl(detail.posterUrl, 240);
  const directors = directorsOf(detail.crew);
  const status: RequestStatus | null = detail.requestStatus ?? null;
  const seasonSuffix =
    detail.kind === 'show' && detail.seasons.length > 0
      ? ` · ${t('content.seasonCount', { count: detail.seasons.length })}`
      : '';

  return (
    <div className="relative min-h-[58vh]">
      <HeroBackdrop
        bg={
          detail.backdropUrl
            ? `url("${detail.backdropUrl}")`
            : `linear-gradient(135deg, ${c1}, ${c2})`
        }
      />

      <button
        type="button"
        onClick={onBack}
        aria-label={t('discover.back')}
        className="absolute left-8 top-6.5 z-3 flex h-10.5 w-10.5 items-center justify-center rounded-full
          border border-white/12 bg-[rgba(10,10,12,.5)] backdrop-blur-sm transition-colors hover:bg-[rgba(10,10,12,.8)]"
      >
        <IconChevronLeft size={20} stroke={2} color="#fff" />
      </button>

      <div className="relative flex flex-wrap items-end gap-10 px-(--gutter-web) pb-9 pt-22.5">
        <div
          className="relative aspect-2/3 w-56 shrink-0 overflow-hidden rounded-[14px] shadow-hero"
          style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
        >
          {poster ? (
            <img
              src={poster}
              alt=""
              draggable={false}
              className="absolute inset-0 h-full w-full object-cover"
            />
          ) : null}
        </div>

        <div className="max-w-170 flex-1 [text-shadow:0_1px_3px_rgba(0,0,0,.5),0_2px_16px_rgba(0,0,0,.55)]">
          <div className="mb-3 text-[12px] font-semibold uppercase tracking-[.18em] text-accent">
            {t(detail.kind === 'show' ? 'discover.kindShow' : 'discover.kindMovie')}
            {seasonSuffix}
          </div>
          <h1 className="mb-4 font-display text-[52px] font-bold leading-none tracking-[-.02em] [text-shadow:0_0_2px_rgba(0,0,0,.55),0_2px_8px_rgba(0,0,0,.55),0_8px_30px_rgba(0,0,0,.6)]">
            {detail.title}
          </h1>

          <div className="mb-4 flex flex-wrap items-center gap-2.5">
            {detail.rating ? (
              <>
                <span className="text-[14px] font-bold text-accent">
                  {detail.rating.toFixed(1)}★
                </span>
                <span className="text-white/40">·</span>
              </>
            ) : null}
            <span className="text-[14px] font-medium text-white/72">{metaLine(detail)}</span>
            {detail.genres.slice(0, 3).map((g) => (
              <span
                key={g}
                className="rounded-full bg-white/8 px-2.5 py-0.5 text-[12px] font-semibold text-white/70"
              >
                {g}
              </span>
            ))}
          </div>

          {directors.length > 0 ? (
            <div className="mb-3 text-[13.5px] text-white/60">
              <span className="font-semibold text-white/80">{t('content.directedBy')}</span>{' '}
              {directors.map((d, i) => (
                <span key={d}>
                  {i > 0 ? ', ' : ''}
                  <button
                    type="button"
                    onClick={() => navigate({ to: '/person/$name', params: { name: d } })}
                    aria-label={t('person.viewWorks', { name: d })}
                    className="cursor-pointer bg-transparent p-0 text-inherit underline-offset-2 transition-colors hover:text-accent hover:underline"
                  >
                    {d}
                  </button>
                </span>
              ))}
            </div>
          ) : null}

          {detail.tagline ? (
            <p className="mb-3 text-[14px] italic text-white/50">{detail.tagline}</p>
          ) : null}
          {detail.overview ? (
            <p className="mb-6 max-w-2xl text-[15.5px] leading-[1.6] text-white/82">
              {detail.overview}
            </p>
          ) : null}

          <div className="flex items-center gap-4">
            <HeroCta
              detail={detail}
              busy={busy}
              status={status}
              onRequest={onRequest}
              onViewLibrary={onViewLibrary}
            />
          </div>
          {error ? (
            <p className="mt-3.5 text-[13.5px] font-semibold text-[#EF8091]">{error}</p>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/** The hero's primary action: View-in-library when owned, the live status chip
 * once requested, else the Request button (which the parent wires to the season
 * sheet for shows). */
function HeroCta({
  detail,
  busy,
  status,
  onRequest,
  onViewLibrary,
}: Readonly<{
  detail: DiscoverDetail;
  busy: boolean;
  status: RequestStatus | null;
  onRequest: () => void;
  onViewLibrary: () => void;
}>) {
  const t = useT();

  if (detail.inLibrary && detail.localId) {
    return (
      <button
        type="button"
        onClick={onViewLibrary}
        className="rounded-xl bg-accent px-6 py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover"
      >
        {t('discover.viewInLibrary')}
      </button>
    );
  }

  if (status && status !== 'denied') {
    return <RequestStatusChip status={status} size="hero" progress={detail.requestProgress} />;
  }

  return (
    <button
      type="button"
      disabled={busy}
      onClick={onRequest}
      className="inline-flex items-center gap-2 rounded-xl bg-accent px-6 py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-60"
    >
      {busy ? (
        <IconLoader2 size={17} stroke={2.4} className="animate-spin" />
      ) : (
        <IconPlus size={17} stroke={2.6} />
      )}
      {detail.kind === 'show' ? t('discover.requestShow') : t('discover.request')}
    </button>
  );
}
