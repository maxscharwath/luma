import { useState } from 'react';

export interface PosterProps {
  title: string;
  genre?: string;
  badge?: string | null;
  /** Two-stop gradient fallback when no artwork is available. */
  colors?: [string, string];
  /** Real poster artwork (WebP) — falls back to the gradient. */
  poster?: string | null;
  progress?: number | null;
  width?: number;
  onClick?: () => void;
}

/**
 * Poster tile. Hover lifts the card and rings it in amber (LUMA design).
 * When real artwork is present the text overlay is hidden (the poster already
 * carries the title) and only reveals on hover; for gradient placeholders it
 * always shows.
 */
export function Poster({
  title,
  genre,
  badge = null,
  colors = ['#3A2E5C', '#0E1430'],
  poster = null,
  progress = null,
  width = 208,
  onClick,
}: Readonly<PosterProps>) {
  const [imgOk, setImgOk] = useState(true);
  const showImg = Boolean(poster) && imgOk;
  const gradient = `linear-gradient(158deg, ${colors[0]} 0%, ${colors[1]} 70%)`;

  return (
    <button
      type="button"
      onClick={onClick}
      style={{ width }}
      className="group block shrink-0 text-left transition-transform duration-200 ease-(--ease-out) hover:-translate-y-1.5 focus:outline-none"
    >
      <div
        className="relative aspect-2/3 overflow-hidden rounded-lg shadow-card transition-shadow duration-200
          group-hover:shadow-[0_0_0_3px_var(--luma-accent),var(--shadow-pop)]
          group-focus-visible:shadow-[0_0_0_3px_var(--luma-accent),var(--shadow-pop)]"
        style={{ background: gradient }}
      >
        {showImg ? (
          <img
            src={poster ?? undefined}
            alt=""
            loading="lazy"
            decoding="async"
            draggable={false}
            onError={() => setImgOk(false)}
            className="absolute inset-0 h-full w-full object-cover"
          />
        ) : null}
        <div className="absolute inset-0 bg-linear-to-b from-black/5 via-transparent to-black/70" />
        {badge ? (
          <span className="absolute right-2.5 top-2.5 rounded bg-[rgba(10,10,12,.6)] px-1.75 py-1 text-[10px] font-bold text-accent">
            {badge}
          </span>
        ) : null}
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
  );
}
