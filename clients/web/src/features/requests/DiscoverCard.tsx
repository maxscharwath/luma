// A TMDB discovery result as a poster tile: real art or gradient, overlaid
// rating + availability/request chip, and a hover "request" affordance.
// Clicks route to the local fiche when owned, else the discover detail.

import { type DiscoverEntry, posterColors, sizedImageUrl } from '@luma/core';
import { useT } from '@luma/ui';
import { IconPlus, IconStarFilled } from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { RequestStatusChip } from '#web/features/requests/RequestStatusChip';

export function DiscoverCard({ entry, width = 208 }: Readonly<{ entry: DiscoverEntry; width?: number }>) {
  const t = useT();
  const navigate = useNavigate();
  const [imgOk, setImgOk] = useState(true);
  const [c1, c2] = posterColors(String(entry.tmdbId));
  const art = sizedImageUrl(entry.posterUrl, width);
  const showImg = Boolean(art) && imgOk;
  const owned = entry.inLibrary && entry.localId;
  const canRequest = !owned && !entry.requestStatus;

  const open = () => {
    if (owned) {
      navigate({
        to: entry.kind === 'show' ? '/show/$id' : '/movie/$id',
        params: { id: entry.localId ?? '' },
      });
    } else {
      navigate({
        to: '/discover/$type/$tmdbId',
        params: { type: entry.kind === 'show' ? 'tv' : 'movie', tmdbId: String(entry.tmdbId) },
      });
    }
  };

  return (
    <div
      style={{ width }}
      className="group/card relative block shrink-0 text-left transition-transform duration-200 ease-(--ease-out) hover:-translate-y-1.5"
    >
      <button type="button" onClick={open} className="block w-full text-left focus:outline-none">
        <div
          className="relative aspect-2/3 overflow-hidden rounded-lg shadow-card transition-shadow duration-200 group-hover/card:shadow-[0_0_0_3px_var(--luma-accent),var(--shadow-pop)]"
          style={{ background: `linear-gradient(158deg, ${c1} 0%, ${c2} 70%)` }}
        >
          {showImg ? (
            <img
              src={art ?? ''}
              alt={entry.title}
              loading="lazy"
              onError={() => setImgOk(false)}
              className="absolute inset-0 h-full w-full object-cover"
            />
          ) : (
            <div className="absolute inset-0 flex items-end p-3">
              <span className="line-clamp-3 text-[15px] font-bold leading-tight text-white/90">
                {entry.title}
              </span>
            </div>
          )}

          {/* top gradient scrim keeps the chips legible over bright art */}
          <div className="pointer-events-none absolute inset-x-0 top-0 h-16 bg-gradient-to-b from-black/55 to-transparent opacity-0 transition-opacity group-hover/card:opacity-100" />

          <div className="absolute left-2 top-2 flex flex-col gap-1.5">
            {owned ? (
              <RequestStatusChip status="available" size="card" />
            ) : entry.requestStatus ? (
              <RequestStatusChip
                status={entry.requestStatus}
                size="card"
                progress={entry.requestProgress}
              />
            ) : null}
          </div>

          {entry.rating ? (
            <span className="absolute right-2 top-2 inline-flex items-center gap-0.5 rounded-full bg-black/55 px-1.5 py-0.5 text-[10.5px] font-bold text-[#F4B642] backdrop-blur-[4px]">
              <IconStarFilled size={9} />
              {entry.rating.toFixed(1)}
            </span>
          ) : null}

          {/* hover request hint (visual only; the click opens the detail where the
              real request action lives) */}
          {canRequest ? (
            <div className="pointer-events-none absolute inset-x-0 bottom-0 flex translate-y-2 items-center justify-center gap-1.5 bg-gradient-to-t from-black/75 to-transparent pb-3 pt-8 text-[12.5px] font-bold text-white opacity-0 transition-all duration-200 group-hover/card:translate-y-0 group-hover/card:opacity-100">
              <IconPlus size={14} stroke={2.6} />
              {t('discover.request')}
            </div>
          ) : null}
        </div>
      </button>
      <div className="mt-2 px-0.5">
        <div className="truncate text-[14px] font-semibold text-text">{entry.title}</div>
        <div className="mt-0.5 flex items-center gap-1.5 text-[12.5px] font-medium text-dim">
          <span>{entry.kind === 'show' ? t('discover.kindShow') : t('discover.kindMovie')}</span>
          {entry.year ? (
            <>
              <span className="text-white/20">·</span>
              <span>{entry.year}</span>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}
