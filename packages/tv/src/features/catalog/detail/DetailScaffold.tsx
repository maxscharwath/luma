import { posterColors } from '@luma/core';
import { useT } from '@luma/ui';
import type { ReactNode } from 'react';
import { badgeClasses, TvArt } from '#tv/shared/TvMedia';

const VEIL =
  'absolute inset-0 bg-[linear-gradient(90deg,#0A0A0C_12%,transparent_68%),linear-gradient(0deg,#0A0A0C_4%,transparent_60%)]';

/**
 * Shared chrome for the Film / Série detail screens: full-bleed backdrop, veil,
 * the overline + title + meta row + synopsis header, and the (deliberately
 * last-focusable) Retour button. Screen-specific actions and extras render as
 * `children` they come before Retour in the DOM, so the first action (Lecture)
 * stays the initial spatial-focus target on mount.
 */
export function TvDetailScaffold({
  id,
  kind,
  title,
  backdrop,
  rating,
  meta,
  badge,
  overview,
  children,
}: Readonly<{
  id: string;
  kind: string;
  title: string;
  backdrop: string | null;
  rating: number | null | undefined;
  meta: string;
  badge: string | null;
  overview: string | null | undefined;
  children: ReactNode;
}>) {
  const t = useT();
  return (
    <div className="fixed inset-0 overflow-hidden bg-bg">
      <TvArt src={backdrop} colors={posterColors(id)} position="50% 18%" />
      <div className={VEIL} />

      <div className="scrollbar-none absolute inset-0 overflow-y-auto px-16 pt-[34vh] pb-16">
        <div className="mb-3.5 font-sans text-[13px] font-bold uppercase tracking-[0.2em] text-accent">
          {kind}
        </div>
        <h1 className="m-0 mb-4 font-display text-[clamp(46px,7.6vh,86px)] font-bold leading-[0.95] tracking-[-0.02em]">
          {title}
        </h1>

        <div className="mb-4.5 flex flex-wrap items-center gap-3.25 font-sans text-[18px] font-semibold text-muted">
          {rating ? (
            <>
              <span className="font-bold text-accent">{rating.toFixed(1)}★</span>
              <span className="text-dim">·</span>
            </>
          ) : null}
          <span>{meta}</span>
          {badge ? <span className={badgeClasses(badge)}>{badge}</span> : null}
        </div>

        {overview ? (
          <p className="m-0 mb-6.5 max-w-170 font-sans text-[20px] leading-[1.5] text-[rgba(244,243,240,0.82)] line-clamp-3">
            {overview}
          </p>
        ) : null}

        {children}
      </div>

      {/* Back is the remote key (wired via useFocusNav onBack), so this is a
          non-focusable hint. It's a self-contained chip with its own scrim so it
          never collides with the title when the page is scrolled under it. */}
      <div className="absolute left-16 top-8 z-10 inline-flex items-center gap-2 rounded-full bg-[rgba(10,10,12,0.55)] px-4 py-2 font-sans text-[14px] font-semibold text-[rgba(244,243,240,0.75)] backdrop-blur-md">
        {t('content.detailBack')}
        <span className="text-[rgba(244,243,240,0.4)]">{t('content.detailBackHint')}</span>
      </div>
    </div>
  );
}
