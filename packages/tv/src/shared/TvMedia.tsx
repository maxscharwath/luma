import { sizedImageUrl } from '@kroma/core';
import { IconPlayerPlayFilled } from '@tabler/icons-react';
import { memo, useEffect, useState } from 'react';

/* Shared class strings. Translucency uses literal rgba() arbitrary values (not
   Tailwind's `/opacity`, which compiles to color-mix() unsupported on the
   Chrome 99–110 / 2024-TV range and not down-levellable when the colour is a
   CSS variable). Solid theme colours map straight to `var(--color-*)`. */

/** Amber primary action button (hero / detail "Lecture"). */
export const TV_PLAY_BTN =
  'inline-flex items-center gap-2.75 cursor-pointer rounded-lg bg-accent px-9 py-4 font-sans text-[19px] font-bold text-accent-ink transition-transform focus:scale-[1.04] disabled:cursor-default disabled:opacity-50';

/** Filled play triangle primary-action / episode-thumb glyph. */
export function PlayGlyph({ size = 22 }: Readonly<{ size?: number }>) {
  return <IconPlayerPlayFilled size={size} />;
}

/** A filled amber check badge, shown top-left of a tile the user has watched. */
export function WatchedBadge({ size = 28 }: Readonly<{ size?: number }>) {
  return (
    <div
      className="absolute left-3 top-3 z-1 flex items-center justify-center rounded-full bg-accent text-accent-ink shadow-card"
      style={{ width: size, height: size }}
    >
      <svg
        width={size * 0.6}
        height={size * 0.6}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M20 6 9 17l-5-5" />
      </svg>
    </div>
  );
}

/** Quality badge chip classes for a badge label (4K/H.265/HDR), or '' for none. */
export function badgeClasses(badge: string | null): string {
  const base = 'rounded-[7px] px-2.75 py-1.25 font-sans text-[13px] font-bold tracking-[0.02em]';
  if (!badge) return '';
  if (badge === 'HDR') return `${base} text-hdr bg-[rgba(199,146,234,0.16)]`;
  if (badge === 'H.265') return `${base} text-h265 bg-[rgba(95,211,196,0.16)]`;
  return `${base} text-accent bg-accent-soft`;
}

/**
 * Key-art fill: a real `<img>` over a deterministic genre gradient. The gradient
 * shows instantly (and stays as the fallback) while the image loads or if it
 * fails so a tile/hero is never blank, matching the cinematic KROMA look.
 */
export function TvArt({
  src,
  colors,
  alt = '',
  position = '50% 30%',
}: Readonly<{
  src: string | null;
  colors: [string, string];
  alt?: string;
  /** object-position for the artwork (heroes favour the upper third). */
  position?: string;
}>) {
  const [ok, setOk] = useState(true);
  // Reset the error flag when the source changes (live catalog/art updates).
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-running on `src` change is the whole point (reset the error flag for the new source); the body reads only setOk.
  useEffect(() => setOk(true), [src]);

  return (
    <div
      aria-hidden={alt ? undefined : true}
      className="absolute inset-0"
      style={{ background: `linear-gradient(158deg, ${colors[0]} 0%, ${colors[1]} 72%)` }}
    >
      {src && ok ? (
        <img
          src={src}
          alt={alt}
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setOk(false)}
          className="absolute inset-0 h-full w-full object-cover"
          style={{ objectPosition: position }}
        />
      ) : null}
    </div>
  );
}

export interface TvCardProps {
  title: string;
  genre?: string;
  /** Landscape key-art (backdrop) URL; falls back to the `colors` gradient. */
  backdrop: string | null;
  colors: [string, string];
  progress?: number | null;
  /** Show the "watched" check badge (top-left). */
  watched?: boolean;
  width?: number;
  onClick?: () => void;
}

/**
 * 16:9 landscape rail tile for the 10-foot home/rows. Focusable for the remote;
 * the amber focus ring comes from the global `[data-focus]:focus` rule.
 */
function TvCardImpl({
  title,
  genre,
  backdrop,
  colors,
  progress = null,
  watched = false,
  width = 328,
  onClick,
}: Readonly<TvCardProps>) {
  return (
    <button
      type="button"
      className="flex-none cursor-pointer rounded-xl border-none bg-transparent p-0 text-left transition-transform focus:scale-[1.06]"
      data-focus=""
      onClick={onClick}
      style={{ width }}
    >
      <div className="relative aspect-video overflow-hidden rounded-xl bg-surface-1 shadow-card [contain-intrinsic-size:328px_185px] [content-visibility:auto]">
        <TvArt src={sizedImageUrl(backdrop, width)} colors={colors} position="50% 28%" />
        <div className="absolute inset-0 bg-linear-to-b from-[rgba(0,0,0,0.05)] from-40% to-[rgba(0,0,0,0.75)]" />
        {watched ? <WatchedBadge /> : null}
        <div className="absolute inset-x-4.5 bottom-4">
          {genre ? (
            <div className="mb-1.25 font-sans text-[12px] font-bold uppercase tracking-widest text-[rgba(255,255,255,0.65)]">
              {genre}
            </div>
          ) : null}
          <div className="text-left font-display text-[24px] font-bold leading-[1.02] text-white">
            {title}
          </div>
        </div>
        {progress != null ? (
          <div className="absolute inset-x-0 bottom-0 h-1.5 bg-[rgba(255,255,255,0.25)]">
            <div className="h-full bg-accent" style={{ width: `${progress}%` }} />
          </div>
        ) : null}
      </div>
    </button>
  );
}

export const TvCard = memo(TvCardImpl);

export interface TvPosterProps {
  title: string;
  /** 2:3 poster art URL; falls back to the `colors` gradient. */
  poster: string | null;
  colors: [string, string];
  /** Show the "watched" check badge (top-left). */
  watched?: boolean;
  /** Resume / series-completion progress bar (%), or null for none. */
  progress?: number | null;
  onClick?: () => void;
  /** Fired when the tile takes focus (D-pad move or pointer click). */
  onFocus?: () => void;
}

/**
 * 2:3 poster tile for the browse grids (Films / Séries). Fills its grid cell and
 * uses `content-visibility:auto` so off-screen tiles in a 1000-item grid skip
 * layout + paint entirely while staying in the DOM for remote focus navigation.
 */
function TvPosterImpl({
  title,
  poster,
  colors,
  watched = false,
  progress = null,
  onClick,
  onFocus,
}: Readonly<TvPosterProps>) {
  return (
    <button
      className="w-full cursor-pointer rounded-lg border-none bg-transparent p-0 transition-transform focus:scale-[1.05]"
      data-focus=""
      type="button"
      onClick={onClick}
      onFocus={onFocus}
    >
      <div className="relative aspect-2/3 overflow-hidden rounded-lg bg-surface-1 shadow-card [contain-intrinsic-size:200px_300px] [content-visibility:auto]">
        <TvArt src={sizedImageUrl(poster, 240)} colors={colors} position="50% 50%" />
        <div className="absolute inset-0 bg-[linear-gradient(170deg,rgba(0,0,0,0.05)_35%,rgba(0,0,0,0.72))]" />
        {watched ? <WatchedBadge size={26} /> : null}
        <div className="absolute inset-x-3.5 bottom-3 text-left font-display text-[18px] font-bold leading-[1.05] text-white">
          {title}
        </div>
        {progress != null ? (
          <div className="absolute inset-x-0 bottom-0 h-1.5 bg-[rgba(255,255,255,0.25)]">
            <div className="h-full bg-accent" style={{ width: `${progress}%` }} />
          </div>
        ) : null}
      </div>
    </button>
  );
}

export const TvPoster = memo(TvPosterImpl);
