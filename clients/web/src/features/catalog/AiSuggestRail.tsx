// The per-title "Suggestions IA" rail on a detail page. The server generates
// these lazily with the LLM connector and caches them, so the first view returns
// `null` (generating) we poll until a section arrives (its items may be empty
// when the model found nothing, in which case we render nothing). Reuses the same
// Poster/Rail as the home + similar rails, and handles movies *and* shows.

import { posterColors, type Section } from '@luma/core';
import { useT } from '@luma/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';
import { Poster, Rail } from '#web/shared/ui';

const HEADING = 'mb-1 px-(--gutter-web) font-display text-[22px] font-bold tracking-[-.02em]';
/** How many times to re-poll while the model is still generating (×6s ≈ 72s). */
const MAX_POLLS = 12;

export function AiSuggestRail({ id }: Readonly<{ id: string }>) {
  const t = useT();
  const { ready, user, client } = useAuth();
  const navigate = useNavigate();
  const [section, setSection] = useState<Section | null>(null);
  const [pending, setPending] = useState(true);

  useEffect(() => {
    if (!ready || !user) return;
    let cancelled = false;
    let tries = 0;
    let timer: ReturnType<typeof setTimeout>;
    setSection(null);
    setPending(true);
    const poll = () => {
      client
        .aiSuggest(id)
        .then((res) => {
          if (cancelled) return;
          if (res) {
            // Terminal: a section (possibly with empty items).
            setSection(res);
            setPending(false);
          } else if (tries++ < MAX_POLLS) {
            timer = setTimeout(poll, 6000); // still generating
          } else {
            setPending(false); // gave up waiting
          }
        })
        .catch(() => {
          if (!cancelled) setPending(false);
        });
    };
    poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [id, ready, user, client]);

  if (section && section.items.length > 0) {
    return (
      <section className="mt-11">
        <h2 className={HEADING}>{section.title}</h2>
        {section.reason ? (
          <p className="mb-4 px-(--gutter-web) text-[14px] text-white/45">{section.reason}</p>
        ) : (
          <div className="mb-3" />
        )}
        <Rail gap={18} padded label={section.title}>
          {section.items.map((entry) =>
            entry.type === 'show' ? (
              <Poster
                key={entry.show.id}
                title={entry.show.title}
                genre={entry.show.metadata?.genres?.[0] ?? t('content.series')}
                colors={posterColors(entry.show.id)}
                poster={client.showPosterFor(entry.show)}
                width={200}
                onClick={() => navigate({ to: '/show/$id', params: { id: entry.show.id } })}
              />
            ) : (
              <Poster
                key={entry.item.id}
                title={entry.item.title}
                genre={entry.item.metadata?.genres?.[0] ?? t('content.film')}
                colors={posterColors(entry.item.id)}
                poster={client.posterFor(entry.item)}
                width={200}
                onClick={() => navigate({ to: '/movie/$id', params: { id: entry.item.id } })}
              />
            ),
          )}
        </Rail>
      </section>
    );
  }

  // Still generating → a subtle placeholder so the user knows it's coming.
  // Terminal-empty or gave up → render nothing.
  if (pending) {
    return (
      <section className="mt-11">
        <h2 className={HEADING}>{t('content.aiSuggestions')}</h2>
        <p className="mt-3 px-(--gutter-web) text-[14px] text-white/40">{t('content.aiSuggestionsLoading')}</p>
      </section>
    );
  }
  return null;
}
