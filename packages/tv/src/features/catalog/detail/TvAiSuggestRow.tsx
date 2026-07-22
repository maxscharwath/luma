import { posterColors, type SectionItem } from '@kroma/core';
import { useAiSuggest, useT } from '@kroma/ui';
import { Box, MediaCard, ProgressRing, Rail, Txt } from '@kroma/ui/kit';
import { useClient, useNav } from '#tv/app/router';

// The "Suggestions IA" rail on a TV detail screen. The shared `useAiSuggest` hook
// polls the lazily-generated section (LLM connector, server-cached); a progress
// ring shows while it generates and cards render once items arrive (empty items or a
// timeout -> nothing). The cards are <Focusable>s, and the focus engine picks up
// whatever is on screen at the time, so the rail becomes navigable the moment it
// appears.

const LABEL = {
  fontWeight: '700' as const,
  fontSize: 15,
  lineHeight: 18,
  letterSpacing: 0.6,
  textTransform: 'uppercase' as const,
  color: 'rgba(244, 243, 240, 0.55)',
};

export function TvAiSuggestRow({ id }: Readonly<{ id: string }>) {
  const t = useT();
  const client = useClient();
  const { go } = useNav();
  const { section, pending, progress } = useAiSuggest(client, id);

  const card = (e: SectionItem) => {
    if (e.type === 'show') {
      const s = e.show;
      return (
        <MediaCard
          key={s.id}
          title={s.title}
          overline={s.metadata?.genres?.[0] ?? t('content.series')}
          art={client.backdropFor(s) ?? client.showPosterFor(s)}
          tint={posterColors(s.id)}
          width={300}
          onPress={() => go('show', { show: s })}
        />
      );
    }
    const m = e.item;
    return (
      <MediaCard
        key={m.id}
        title={m.title}
        overline={m.metadata?.genres?.[0] ?? t('content.film')}
        art={client.backdropFor(m) ?? client.posterFor(m)}
        tint={posterColors(m.id)}
        width={300}
        onPress={() => go('movie', { item: m })}
      />
    );
  };

  if (section && section.items.length > 0) {
    return (
      <Box mt={40} gap={16}>
        <Txt style={LABEL}>{section.title}</Txt>
        {section.reason ? (
          <Txt style={{ fontSize: 16, lineHeight: 22, maxWidth: 680 }} color="textMuted">
            {section.reason}
          </Txt>
        ) : null}
        {/* The rail already pads itself by the 10-foot gutter; this one sits
            inside a detail column that has its own padding, so it opts out. */}
        <Rail inset={0} gap={24}>
          {section.items.map(card)}
        </Rail>
      </Box>
    );
  }

  // Still generating -> a subtle hint; terminal-empty or gave up -> nothing.
  if (pending) {
    return (
      <Box mt={40} gap={16}>
        <Txt style={LABEL}>{t('content.aiSuggestions')}</Txt>
        <Box row align="center" gap={16}>
          <ProgressRing
            value={progress}
            size={26}
            stroke={3}
            track="rgba(244, 243, 240, 0.15)"
            fill="rgba(244, 243, 240, 0.7)"
          />
          <Txt style={{ fontSize: 16 }} color="textDim">
            {t('content.aiSuggestionsLoading')}
          </Txt>
        </Box>
      </Box>
    );
  }
  return null;
}
