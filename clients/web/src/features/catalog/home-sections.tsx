// Server-generated home: an ordered list of localized sections (For You,
// "Because you watched …", themed rows, trending, recently added). The server
// assembles, orders, de-duplicates and localizes everything, so this is a thin
// renderer. Per-user, so it loads client-side once a session is hydrated,
// showing rail skeletons while it loads and nothing once it's empty.

import { useSuspenseQuery } from '@tanstack/react-query';
import { Suspense } from 'react';
import { SectionPoster } from '#web/features/catalog/cards';
import { useAuth } from '#web/shared/lib/auth';
import { userQueries } from '#web/shared/lib/queries';
import { Rail, RailSkeleton } from '#web/shared/ui';

const SECTION_TITLE = 'mb-5 mt-10 font-display text-[22px] font-bold tracking-[-.02em] text-text';

export function HomeSections() {
  const { user, ready } = useAuth();
  if (!ready || !user) return null;
  return (
    <Suspense
      fallback={
        <>
          <RailSkeleton />
          <RailSkeleton />
        </>
      }
    >
      <Sections />
    </Suspense>
  );
}

function Sections() {
  const { data: sections } = useSuspenseQuery(userQueries.home());

  if (sections.length === 0) return null;

  return (
    <>
      {sections.map((section) => {
        if (section.items.length === 0) return null;
        return (
          <section key={section.id}>
            <h2 className={SECTION_TITLE}>{section.title}</h2>
            <Rail label={section.title}>
              {section.items.map((entry) => (
                <SectionPoster
                  key={entry.type === 'show' ? entry.show.id : entry.item.id}
                  entry={entry}
                />
              ))}
            </Rail>
          </section>
        );
      })}
    </>
  );
}
