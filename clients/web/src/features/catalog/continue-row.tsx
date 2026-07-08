// "Reprendre la lecture" rail. Unlike the catalogue rails (SSR-loaded, public),
// this is per-user so it loads client-side once a session is hydrated, then
// renders resumable items with a progress bar straight to the player.

import { type ContinueItem, episodeTag, posterColors } from '@luma/core';
import { useT } from '@luma/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { Poster, Rail } from '#web/shared/ui';
import { useAuth } from '#web/shared/lib/auth';

const SECTION_TITLE = 'mb-5 mt-10 font-display text-[22px] font-bold tracking-[-.02em] text-text';

export function ContinueRow() {
  const t = useT();
  const { user, ready, client } = useAuth();
  const [items, setItems] = useState<ContinueItem[]>([]);
  const navigate = useNavigate();

  useEffect(() => {
    if (!ready || !user) {
      setItems([]);
      return;
    }
    let cancelled = false;
    client
      .continueWatching()
      .then((r) => {
        if (!cancelled) setItems(r);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [ready, user, client]);

  if (items.length === 0) return null;

  return (
    <section>
      <h2 className={SECTION_TITLE}>{t('content.continueWatching')}</h2>
      <Rail label={t('content.continueWatching')}>
        {items.map(({ item, positionMs, durationMs }) => {
          const dur = durationMs ?? item.durationMs ?? 0;
          const pct = dur > 0 ? Math.min(100, Math.round((positionMs / dur) * 100)) : 0;
          const label =
            item.kind === 'episode' && item.showTitle
              ? `${item.showTitle} · ${episodeTag(item)}`
              : t('content.film');
          return (
            <Poster
              key={item.id}
              title={item.title}
              genre={label}
              colors={posterColors(item.id)}
              poster={client.posterFor(item)}
              progress={pct}
              onClick={() => navigate({ to: '/watch/$id', params: { id: item.id } })}
            />
          );
        })}
      </Rail>
    </section>
  );
}
