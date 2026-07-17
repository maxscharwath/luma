import { sizedImageUrl } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconCheck } from '@tabler/icons-react';
import { useState } from 'react';

export interface PosterProps {
  title: string;
  genre?: string;
  /** Two-stop gradient fallback when no artwork is available. */
  colors?: [string, string];
  /** Real poster artwork (WebP) falls back to the gradient. */
  poster?: string | null;
  progress?: number | null;
  /** When set, renders the "watched" marker/toggle: true = seen (persistent
   * check badge), false = unseen (check appears on hover). Omit to hide it. */
  watched?: boolean | null;
  /** Toggle the watched flag. Required for the marker to be interactive. */
  onToggleWatched?: () => void;
  /** Fixed tile width in px; omit for the fluid default (`--card-w`, which
   * scales from phone to desktop). */
  width?: number;
  onClick?: () => void;
}

/**
 * Poster tile. Hover lifts the card and rings it in amber (KROMA design).
 * When real artwork is present the text overlay is hidden (the poster already
 * carries the title) and only reveals on hover; for gradient placeholders it
 * always shows.
 *
 * The tile is a `<div>` wrapper (not a `<button>`) so the watched toggle can be
 * a real, focusable `<button>` sibling without nesting interactive elements.
 */
export function Poster({
  title,
  genre,
  colors = ['#3A2E5C', '#0E1430'],
  poster = null,
  progress = null,
  watched = null,
  onToggleWatched,
  width,
  onClick,
}: Readonly<PosterProps>) {
  const t = useT();
  const [imgOk, setImgOk] = useState(true);
  const showImg = Boolean(poster) && imgOk;
  const gradient = `linear-gradient(158deg, ${colors[0]} 0%, ${colors[1]} 70%)`;
  const showToggle = watched != null && Boolean(onToggleWatched);

  return (
    <div
      style={{ width: width ?? 'var(--card-w)' }}
      className="group relative block shrink-0 text-left transition-transform duration-200 ease-(--ease-out) hover:-translate-y-1.5"
    >
      <button type="button" onClick={onClick} className="block w-full text-left focus:outline-none">
        <div
          className="relative aspect-2/3 overflow-hidden rounded-lg shadow-card transition-shadow duration-200
            group-hover:shadow-[0_0_0_3px_var(--kroma-accent),var(--shadow-pop)]
            group-focus-within:shadow-[0_0_0_3px_var(--kroma-accent),var(--shadow-pop)]"
          style={{ background: gradient }}
        >
          {showImg ? (
            <img
              src={sizedImageUrl(poster, width ?? 208) ?? undefined}
              alt=""
              loading="lazy"
              decoding="async"
              draggable={false}
              onError={() => setImgOk(false)}
              className="absolute inset-0 h-full w-full object-cover"
            />
          ) : null}
          <div className="absolute inset-0 bg-linear-to-b from-black/5 via-transparent to-black/70" />
          <div
            className={`absolute inset-x-3.5 bottom-3.5 ${
              showImg ? 'opacity-0 transition-opacity duration-200 group-hover:opacity-100' : ''
            }`}
          >
            {genre ? (
              <div className="mb-1 text-[10px] font-bold uppercase tracking-[.12em] text-white/60">
                {genre}
              </div>
            ) : null}
            <div className="font-display text-[20px] font-bold text-white">{title}</div>
          </div>
          {progress != null ? (
            <div className="absolute inset-x-0 bottom-0 h-1.25 bg-white/20">
              <div className="h-full bg-accent" style={{ width: `${progress}%` }} />
            </div>
          ) : null}
        </div>
      </button>
      {showToggle ? (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onToggleWatched?.();
          }}
          aria-pressed={watched ?? false}
          aria-label={watched ? t('content.markUnwatched') : t('content.markWatched')}
          title={watched ? t('content.watched') : t('content.markWatched')}
          className={`absolute left-2.5 top-2.5 z-2 flex h-7 w-7 items-center justify-center rounded-full border backdrop-blur-sm transition-all duration-150
            ${
              watched
                ? 'border-accent bg-accent text-black opacity-100'
                : 'border-white/40 bg-[rgba(10,10,12,.55)] text-white opacity-0 hover:!bg-[rgba(10,10,12,.85)] group-hover:opacity-100 group-focus-within:opacity-100'
            }`}
        >
          <IconCheck size={15} stroke={3} />
        </button>
      ) : null}
    </div>
  );
}
