import { posterColors, type SectionItem } from '@kroma/core';
import { ProgressRing, useAiSuggest, useT } from '@kroma/ui';
import { useClient, useNav } from '#tv/app/router';
import { TvCard } from '#tv/shared/TvMedia';

// The "Suggestions IA" rail on a TV detail screen. The shared `useAiSuggest` hook
// polls the lazily-generated section (LLM connector, server-cached); a progress
// ring shows while it generates and cards render once items arrive (empty items or
// a timeout → nothing). Cards carry `data-focus`, and the focus engine re-queries
// the DOM on every move, so the rail becomes navigable the moment it appears.

const LABEL =
  'mb-4 font-sans text-[15px] font-bold uppercase tracking-[0.04em] text-[rgba(244,243,240,0.55)]';

export function TvAiSuggestRow({ id }: Readonly<{ id: string }>) {
  const t = useT();
  const client = useClient();
  const { go } = useNav();
  const { section, pending, progress } = useAiSuggest(client, id);

  const card = (e: SectionItem) => {
    if (e.type === 'show') {
      const s = e.show;
      return (
        <TvCard
          key={s.id}
          title={s.title}
          genre={s.metadata?.genres?.[0] ?? t('content.series')}
          backdrop={client.backdropFor(s) ?? client.showPosterFor(s)}
          colors={posterColors(s.id)}
          width={300}
          onClick={() => go('show', { show: s })}
        />
      );
    }
    const m = e.item;
    return (
      <TvCard
        key={m.id}
        title={m.title}
        genre={m.metadata?.genres?.[0] ?? t('content.film')}
        backdrop={client.backdropFor(m) ?? client.posterFor(m)}
        colors={posterColors(m.id)}
        width={300}
        onClick={() => go('movie', { item: m })}
      />
    );
  };

  if (section && section.items.length > 0) {
    return (
      <div className="mt-10">
        <div className={LABEL}>{section.title}</div>
        {section.reason ? (
          <p className="mb-4 max-w-170 font-sans text-[16px] leading-[1.4] text-[rgba(244,243,240,0.6)]">
            {section.reason}
          </p>
        ) : null}
        <div className="scrollbar-none flex gap-6 overflow-x-auto py-4">
          {section.items.map(card)}
        </div>
      </div>
    );
  }

  // Still generating → a subtle hint; terminal-empty or gave up → nothing.
  if (pending) {
    return (
      <div className="mt-10">
        <div className={LABEL}>{t('content.aiSuggestions')}</div>
        <div className="flex items-center gap-4">
          <ProgressRing
            value={progress}
            size={26}
            stroke={3}
            track="rgba(244,243,240,0.15)"
            fill="rgba(244,243,240,0.7)"
          />
          <span className="font-sans text-[16px] text-[rgba(244,243,240,0.4)]">
            {t('content.aiSuggestionsLoading')}
          </span>
        </div>
      </div>
    );
  }
  return null;
}
