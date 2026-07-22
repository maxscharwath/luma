import {
  episodeTag,
  formatRuntime,
  type KromaClient,
  type MediaItem,
  posterColors,
  qualityBadge,
  qualityBadgeForVideo,
  type Section,
  type SectionItem,
  type Show,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  Badge,
  Box,
  Button,
  gradient,
  Img,
  MediaCard,
  qualityTone,
  Rail,
  Txt,
  tintGradient,
  useFocusNav,
} from '@kroma/ui/kit';
import { useCallback, useEffect, useMemo } from 'react';
import { ScrollView } from 'react-native';
import { useConnection } from '#tv/app/providers/connection';
import { useContinue } from '#tv/app/providers/continue';
import { useMyList } from '#tv/app/providers/mylist';
import { useRecommend } from '#tv/app/providers/recommend';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav } from '#tv/app/router';
import { HintBar } from '#tv/features/catalog/home/HintBar';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';

const RAIL_LIMIT = 20;

// Two layers rather than one comma-separated background-image: a multi-value
// background is a CSS-only luxury React Native's gradient support lacks.
const HERO_VEIL_HORIZONTAL = 'linear-gradient(90deg, #0A0A0C 5%, transparent 60%)';
const HERO_VEIL_VERTICAL = 'linear-gradient(0deg, #0A0A0C 1%, transparent 48%)';

// The design sizes the hero with viewport units (64vh, min 520px) and its title
// with clamp(42px, 7.6vh, 82px). On the fixed 1920x1080 stage those resolve to
// constants, and a vh would mean something different on each of the four targets.
const HERO_HEIGHT = 691;
const HERO_EMPTY_HEIGHT = 432;
const HERO_TITLE = {
  fontSize: 82,
  lineHeight: 79,
  fontWeight: '700' as const,
  letterSpacing: -1.64,
};

const FEATURED_LABEL = {
  fontSize: 14,
  fontWeight: '700' as const,
  letterSpacing: 3.36,
  textTransform: 'uppercase' as const,
};

const ROW_TITLE = {
  fontSize: 28,
  lineHeight: 30,
  fontWeight: '700' as const,
  letterSpacing: -0.56,
};

interface Row {
  key: string;
  title: string;
  cards: React.ReactNode[];
}

// A recommendation row entry is a movie *or* a show (the server mixes them).
const entryId = (e: SectionItem): string => (e.type === 'show' ? e.show.id : e.item.id);
const entryMetadata = (e: SectionItem) => (e.type === 'show' ? e.show.metadata : e.item.metadata);

interface HeroInfo {
  hero: SectionItem | null;
  heroId: string | null;
  heroMeta: ReturnType<typeof entryMetadata> | null;
  heroBackdrop: string | null;
  heroBadge: string | null;
}

/** The featured spotlight: the server's daily multi-signal pick
 * (`/api/home/featured`), falling back to the first entry of the top server
 * section, then to the first catalog movie so the hero is never empty, plus its
 * resolved backdrop art and quality badge. */
function computeHero(
  featured: SectionItem | null,
  sections: Section[],
  movies: MediaItem[],
  client: KromaClient,
): HeroInfo {
  const hero: SectionItem | null =
    featured ?? sections[0]?.items[0] ?? (movies[0] ? { type: 'movie', item: movies[0] } : null);
  const heroId = hero ? entryId(hero) : null;
  const heroMeta = hero ? entryMetadata(hero) : null;
  let heroBackdrop: string | null = null;
  if (hero) {
    heroBackdrop =
      hero.type === 'show'
        ? (client.backdropFor(hero.show) ?? client.showPosterFor(hero.show))
        : (client.backdropFor(hero.item) ?? client.posterFor(hero.item));
  }
  let heroBadge: string | null = null;
  if (hero) {
    heroBadge =
      hero.type === 'show' ? qualityBadgeForVideo(hero.show.video) : qualityBadge(hero.item);
  }
  return { hero, heroId, heroMeta, heroBackdrop, heroBadge };
}

/** The 10-foot home a cinematic hero over a vertical stack of horizontal rails
 * (Reprendre / Films / Séries previews). Films, Séries and Search live on their
 * own screens via the shared top nav. */
export function TvHome() {
  const { movies, shows } = useConnection();
  const { items: continueItems, refresh: refreshContinue } = useContinue();
  const { sections, featured } = useRecommend();
  const { has: isWatched, refresh: refreshWatched } = useWatched();
  const { refresh: refreshMyList } = useMyList();
  const { go } = useNav();
  const client = useClient();
  const t = useT();
  useEffect(() => refreshContinue(), [refreshContinue]);
  // Re-pull the watched + my-list sets on entry so a title finished in the player
  // (auto-marked) or added on another device shows up the moment we land on Home.
  useEffect(() => refreshWatched(), [refreshWatched]);
  useEffect(() => refreshMyList(), [refreshMyList]);
  useFocusNav({});

  const onSelectMovie = useCallback((m: MediaItem) => go('movie', { item: m }), [go]);
  const onSelectShow = useCallback((s: Show) => go('show', { show: s }), [go]);
  const onPlay = useCallback((m: MediaItem) => go('player', { item: m }), [go]);
  // Open a recommendation entry: play page for movies, detail for shows.
  const onSelectEntry = useCallback(
    (e: SectionItem) => (e.type === 'show' ? onSelectShow(e.show) : onSelectMovie(e.item)),
    [onSelectMovie, onSelectShow],
  );

  // Render one rail entry (movie or show) as a 16:9 card.
  const entryCard = useCallback(
    (key: string, e: SectionItem): React.ReactNode => {
      if (e.type === 'show') {
        const s = e.show;
        return (
          <MediaCard
            key={`${key}-${s.id}`}
            title={s.title}
            overline={s.metadata?.genres?.[0] ?? t('content.series')}
            art={client.backdropFor(s) ?? client.showPosterFor(s)}
            tint={posterColors(s.id)}
            watched={isWatched(s.id)}
            progress={s.progress == null ? null : s.progress / 100}
            width={330}
            onPress={() => onSelectShow(s)}
          />
        );
      }
      const m = e.item;
      return (
        <MediaCard
          key={`${key}-${m.id}`}
          title={m.title}
          overline={m.metadata?.genres?.[0] ?? t('content.film')}
          art={client.backdropFor(m) ?? client.posterFor(m)}
          tint={posterColors(m.id)}
          watched={isWatched(m.id)}
          width={330}
          onPress={() => onSelectMovie(m)}
        />
      );
    },
    [client, onSelectMovie, onSelectShow, isWatched, t],
  );

  // One 16:9 rail per server section: empty list in → null out, so the home drops
  // it. `title` arrives already localized from the server rendered as-is.
  const mediaRow = useCallback(
    (key: string, title: string, items: SectionItem[]): Row | null =>
      items.length
        ? { key, title, cards: items.slice(0, RAIL_LIMIT).map((e) => entryCard(key, e)) }
        : null,
    [entryCard],
  );

  // Featured spotlight (hero) + its backdrop art and quality badge, computed once.
  const { hero, heroId, heroMeta, heroBackdrop, heroBadge } = computeHero(
    featured,
    sections,
    movies,
    client,
  );

  const rows = useMemo<Row[]>(() => {
    const continueRow: Row | null = continueItems.length
      ? {
          key: 'continue',
          title: t('content.continueWatching'),
          cards: continueItems.map(({ item, positionMs, durationMs }) => {
            const dur = durationMs ?? item.durationMs ?? 0;
            const pct = dur > 0 ? Math.min(100, Math.round((positionMs / dur) * 100)) : 0;
            const genre =
              item.kind === 'episode' && item.showTitle
                ? `${item.showTitle} · ${episodeTag(item)}`
                : t('content.film');
            return (
              <MediaCard
                key={`continue-${item.id}`}
                title={item.title}
                overline={genre}
                art={client.backdropFor(item) ?? client.posterFor(item)}
                tint={posterColors(item.id)}
                progress={pct / 100}
                width={330}
                onPress={() => onPlay(item)}
              />
            );
          }),
        }
      : null;
    // One rail per server section, in the server's order. The hero is picked
    // independently of the sections, so drop it from every row to avoid showing
    // the same title twice (the server already de-dupes rows against each other).
    const sectionRows = sections.map((section) =>
      mediaRow(
        section.id,
        section.title,
        heroId ? section.items.filter((e) => entryId(e) !== heroId) : section.items,
      ),
    );
    const showRow: Row | null = shows.length
      ? {
          key: 'series',
          title: t('nav.series'),
          cards: shows
            .slice(0, RAIL_LIMIT)
            .map((s) => (
              <MediaCard
                key={s.id}
                title={s.title}
                overline={
                  s.metadata?.genres?.[0] ?? t('content.seasonCount', { count: s.seasonCount })
                }
                art={client.backdropFor(s) ?? client.showPosterFor(s)}
                tint={posterColors(s.id)}
                watched={isWatched(s.id)}
                progress={s.progress == null ? null : s.progress / 100}
                width={330}
                onPress={() => onSelectShow(s)}
              />
            )),
        }
      : null;
    return [continueRow, ...sectionRows, showRow].filter((r): r is Row => r !== null);
  }, [
    shows,
    continueItems,
    sections,
    heroId,
    mediaRow,
    client,
    onPlay,
    onSelectShow,
    isWatched,
    t,
  ]);

  return (
    <Box fill bg="bg" overflow="hidden">
      <ScrollView
        style={{ flex: 1, minHeight: 0 }}
        contentContainerStyle={{ paddingBottom: 40 }}
        showsVerticalScrollIndicator={false}
      >
        {hero && heroId ? (
          <Box h={HERO_HEIGHT}>
            <Img
              src={heroBackdrop}
              background={tintGradient(posterColors(heroId))}
              position="50% 22%"
              priority
              fill
            />
            <Box fill pointerEvents="none" style={gradient(HERO_VEIL_HORIZONTAL)} />
            <Box fill pointerEvents="none" style={gradient(HERO_VEIL_VERTICAL)} />
            <Box absolute left={64} bottom={36} z={2} maxW={820}>
              <Txt style={FEATURED_LABEL} color="accent">
                {t('content.featured')}
              </Txt>
              <Txt variant="hero" style={[HERO_TITLE, { marginTop: 16, marginBottom: 14 }]}>
                {hero.type === 'show' ? hero.show.title : hero.item.title}
              </Txt>
              <Box row wrap align="center" gap={12} mb={14}>
                {heroMeta?.rating ? (
                  <>
                    <Txt style={{ fontSize: 17, fontWeight: '700' }} color="accent">
                      {`${heroMeta.rating.toFixed(1)}\u2605`}
                    </Txt>
                    <Txt style={{ fontSize: 17, fontWeight: '600' }} color="textDim">
                      ·
                    </Txt>
                  </>
                ) : null}
                <Txt style={{ fontSize: 17, fontWeight: '600' }} color="textMuted">
                  {heroLine(hero)}
                </Txt>
                {heroBadge ? <Badge tone={qualityTone(heroBadge)}>{heroBadge}</Badge> : null}
              </Box>
              {heroMeta?.overview ? (
                <Txt
                  lines={3}
                  style={{ fontSize: 20, lineHeight: 30, maxWidth: 720, marginBottom: 22 }}
                  color="rgba(244, 243, 240, 0.82)"
                >
                  {heroMeta.overview}
                </Txt>
              ) : null}
              <Box row gap={18}>
                <Button
                  size="tv"
                  icon="player-play-filled"
                  label={hero.type === 'movie' ? t('player.play') : t('content.moreInfo')}
                  onPress={() =>
                    hero.type === 'movie' ? onPlay(hero.item) : onSelectShow(hero.show)
                  }
                />
                <Button
                  size="tv"
                  variant="outline"
                  label={t('content.moreInfo')}
                  onPress={() => onSelectEntry(hero)}
                  style={{ paddingHorizontal: 34 }}
                />
              </Box>
            </Box>
          </Box>
        ) : (
          <Box h={HERO_EMPTY_HEIGHT} />
        )}

        {rows.map((row) => (
          <Box key={row.key} mb={8} mt={18}>
            <Rail title={row.title} titleStyle={ROW_TITLE} gap={24}>
              {row.cards}
            </Rail>
          </Box>
        ))}
      </ScrollView>

      <TvTopNav active="home" />

      <HintBar browseKey="content.hintBrowse" />
    </Box>
  );
}

/** Hero meta line year · runtime · genre (quality lives in the badge). Shows
 * have no runtime, so it's just year · genre. */
function heroLine(e: SectionItem): string {
  if (e.type === 'show') {
    return [e.show.year ? String(e.show.year) : null, e.show.metadata?.genres?.[0]]
      .filter(Boolean)
      .join(' · ');
  }
  const m = e.item;
  return [m.year ? String(m.year) : null, formatRuntime(m.durationMs), m.metadata?.genres?.[0]]
    .filter(Boolean)
    .join(' · ');
}
