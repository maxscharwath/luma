// The per-title "Suggestions IA" rail on a detail page. The shared `useAiSuggest`
// hook polls the lazily-generated section (LLM connector, server-cached); while it
// generates we show a subtle progress ring, and once items arrive we render them
// in the same Poster/Rail as the home + similar rails (movies *and* shows). Empty
// items or a timeout → render nothing.

import { ProgressRing, useAiSuggest, useT } from '@kroma/ui';
import { SectionPoster } from '#web/features/catalog/cards';
import { useAuth } from '#web/shared/lib/auth';
import { Rail } from '#web/shared/ui';

const HEADING = 'mb-1 px-(--gutter-web) font-display text-[22px] font-bold tracking-[-.02em]';

export function AiSuggestRail({ id }: Readonly<{ id: string }>) {
  const t = useT();
  const { ready, user, client } = useAuth();
  const { section, pending, progress } = useAiSuggest(client, id, { active: ready && !!user });

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
          {section.items.map((entry) => (
            <SectionPoster
              key={entry.type === 'show' ? entry.show.id : entry.item.id}
              entry={entry}
            />
          ))}
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
        <div className="mt-3 flex items-center gap-3 px-(--gutter-web)">
          <ProgressRing value={progress} />
          <span className="text-[14px] text-white/40">{t('content.aiSuggestionsLoading')}</span>
        </div>
      </section>
    );
  }
  return null;
}
