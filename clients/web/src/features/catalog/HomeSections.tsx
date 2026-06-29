// Server-generated home: an ordered list of localized sections (For You,
// "Because you watched …", themed rows, trending, recently added). The server
// assembles, orders, de-duplicates and localizes everything, so this is a thin
// renderer. Per-user, so like ForYouRow it loads client-side once a session is
// hydrated, and renders nothing until there's something to show.

import { posterColors, type Section } from '@luma/core';
import { useT } from '@luma/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';
import { Poster, Rail } from '#web/shared/ui';

const SECTION_TITLE = 'mb-5 mt-10 font-display text-[22px] font-bold tracking-[-.02em] text-text';

export function HomeSections() {
  const t = useT();
  const { user, ready, client } = useAuth();
  const [sections, setSections] = useState<Section[]>([]);
  const navigate = useNavigate();

  useEffect(() => {
    if (!ready || !user) {
      setSections([]);
      return;
    }
    let cancelled = false;
    client
      .home()
      .then((r) => {
        if (!cancelled) setSections(r);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [ready, user, client]);

  if (sections.length === 0) return null;

  return (
    <>
      {sections.map((section) => {
        if (section.items.length === 0) return null;
        return (
          <section key={section.id}>
            <h2 className={SECTION_TITLE}>{section.title}</h2>
            <Rail label={section.title}>
              {section.items.map((item) => (
                <Poster
                  key={item.id}
                  title={item.title}
                  genre={item.metadata?.genres?.[0] ?? t('content.film')}
                  colors={posterColors(item.id)}
                  poster={client.posterFor(item)}
                  onClick={() => navigate({ to: '/movie/$id', params: { id: item.id } })}
                />
              ))}
            </Rail>
          </section>
        );
      })}
    </>
  );
}
