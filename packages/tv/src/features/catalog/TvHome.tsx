import {
  episodeTag,
  formatRuntime,
  type MediaItem,
  posterColors,
  qualityBadge,
  qualityBadgeForVideo,
  type SectionItem,
  type Show,
} from '@luma/core';
import { useT } from '@luma/ui';
import { useCallback, useEffect, useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useContinue } from '#tv/app/providers/continue';
import { useMyList } from '#tv/app/providers/mylist';
import { useRecommend } from '#tv/app/providers/recommend';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { badgeClasses, PlayGlyph, TvArt, TvCard } from '#tv/shared/TvMedia';

const RAIL_LIMIT = 20;
const HERO_VEIL =
  'absolute inset-0 bg-[linear-gradient(90deg,#0A0A0C_5%,transparent_60%),linear-gradient(0deg,#0A0A0C_1%,transparent_48%)]';

interface Row {
  key: string;
  title: string;
  cards: React.ReactNode[];
}

// A recommendation row entry is a movie *or* a show (the server mixes them).
const entryId = (e: SectionItem): string => (e.type === 'show' ? e.show.id : e.item.id);
const entryMetadata = (e: SectionItem) => (e.type === 'show' ? e.show.metadata : e.item.metadata);

/** The 10-foot home a cinematic hero over a vertical stack of horizontal rails
 * (Reprendre / Films / Séries previews). Films, Séries and Search live on their
 * own screens via the shared top nav. */
export function TvHome() {
  const { movies, shows } = useConnection();
  const { items: continueItems, refresh: refreshContinue } = useContinue();
  const { sections } = useRecommend();
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
          <TvCard
            key={`${key}-${s.id}`}
            title={s.title}
            genre={s.metadata?.genres?.[0] ?? t('content.series')}
            backdrop={client.backdropFor(s) ?? client.showPosterFor(s)}
            colors={posterColors(s.id)}
            watched={isWatched(s.id)}
            progress={s.progress ?? null}
            width={330}
            onClick={() => onSelectShow(s)}
          />
        );
      }
      const m = e.item;
      return (
        <TvCard
          key={`${key}-${m.id}`}
          title={m.title}
          genre={m.metadata?.genres?.[0] ?? t('content.film')}
          backdrop={client.backdropFor(m) ?? client.posterFor(m)}
          colors={posterColors(m.id)}
          watched={isWatched(m.id)}
          width={330}
          onClick={() => onSelectMovie(m)}
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

  // Featured spotlight: the first entry of the top server section (For You),
  // falling back to the most prominent catalog movie so the hero is never empty.
  const hero: SectionItem | null =
    sections[0]?.items[0] ?? (movies[0] ? { type: 'movie', item: movies[0] } : null);
  const heroId = hero ? entryId(hero) : null;
  const heroMeta = hero ? entryMetadata(hero) : null;

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
              <TvCard
                key={`continue-${item.id}`}
                title={item.title}
                genre={genre}
                backdrop={client.backdropFor(item) ?? client.posterFor(item)}
                colors={posterColors(item.id)}
                progress={pct}
                width={330}
                onClick={() => onPlay(item)}
              />
            );
          }),
        }
      : null;
    // One rail per server section, in the server's order. The top section is
    // "For You"; its first entry is the hero, so drop it there to avoid showing
    // it twice (the server already de-dupes across the other rows).
    const sectionRows = sections.map((section, i) =>
      mediaRow(
        section.id,
        section.title,
        i === 0 && heroId ? section.items.filter((e) => entryId(e) !== heroId) : section.items,
      ),
    );
    const showRow: Row | null = shows.length
      ? {
          key: 'series',
          title: t('nav.series'),
          cards: shows
            .slice(0, RAIL_LIMIT)
            .map((s) => (
              <TvCard
                key={s.id}
                title={s.title}
                genre={
                  s.metadata?.genres?.[0] ?? t('content.seasonCount', { count: s.seasonCount })
                }
                backdrop={client.backdropFor(s) ?? client.showPosterFor(s)}
                colors={posterColors(s.id)}
                watched={isWatched(s.id)}
                progress={s.progress ?? null}
                width={330}
                onClick={() => onSelectShow(s)}
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

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-bg">
      <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto pb-10">
        {hero && heroId ? (
          <section className="relative h-[64vh] min-h-[520px]">
            <TvArt src={heroBackdrop} colors={posterColors(heroId)} position="50% 22%" />
            <div className={HERO_VEIL} />
            <div className="absolute bottom-9 left-16 z-2 max-w-205">
              <div className="mb-4 font-sans text-[14px] font-bold uppercase tracking-[0.24em] text-accent">
                {t('content.featured')}
              </div>
              <h1 className="m-0 mb-3.5 font-display text-[clamp(42px,7.6vh,82px)] font-bold leading-[0.96] tracking-[-0.02em]">
                {hero.type === 'show' ? hero.show.title : hero.item.title}
              </h1>
              <div className="mb-3.5 flex flex-wrap items-center gap-3 font-sans text-[17px] font-semibold text-muted">
                {heroMeta?.rating ? (
                  <>
                    <span className="font-bold text-accent">{heroMeta.rating.toFixed(1)}★</span>
                    <span className="text-dim">·</span>
                  </>
                ) : null}
                <span>{heroLine(hero)}</span>
                {heroBadge ? <span className={badgeClasses(heroBadge)}>{heroBadge}</span> : null}
              </div>
              {heroMeta?.overview ? (
                <p className="m-0 mb-5.5 max-w-180 font-sans text-[clamp(15px,2.3vh,20px)] leading-[1.5] text-[rgba(244,243,240,0.82)] line-clamp-3">
                  {heroMeta.overview}
                </p>
              ) : null}
              <div className="flex gap-4.5">
                <button
                  className="inline-flex cursor-pointer items-center gap-3 rounded-[13px] bg-accent px-10 py-4.5 font-sans text-[20px] font-bold text-accent-ink transition-transform focus:scale-[1.04]"
                  data-focus=""
                  type="button"
                  onClick={() =>
                    hero.type === 'movie' ? onPlay(hero.item) : onSelectShow(hero.show)
                  }
                >
                  <PlayGlyph />
                  {hero.type === 'movie' ? t('player.play') : t('content.moreInfo')}
                </button>
                <button
                  className="inline-flex cursor-pointer items-center gap-3 rounded-[13px] border border-[rgba(255,255,255,0.2)] bg-[rgba(255,255,255,0.12)] px-8.5 py-4.5 font-sans text-[20px] font-semibold text-text transition-transform focus:scale-[1.04]"
                  data-focus=""
                  type="button"
                  onClick={() => onSelectEntry(hero)}
                >
                  {t('content.moreInfo')}
                </button>
              </div>
            </div>
          </section>
        ) : (
          <div className="h-[40vh]" />
        )}

        {rows.map((row) => (
          <div key={row.key} className="mb-2">
            <h2 className="mt-4.5 mb-4 px-16 font-display text-[28px] font-bold tracking-[-0.02em]">
              {row.title}
            </h2>
            <div className="scrollbar-none flex gap-6 overflow-x-auto px-16 py-8">{row.cards}</div>
          </div>
        ))}
      </div>

      <TvTopNav active="home" />

      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center gap-7.5 bg-[linear-gradient(0deg,rgba(10,10,12,0.8),transparent)] p-4 font-sans text-[13px] font-semibold text-dim">
        <span>{t('content.hintBrowse')}</span>
        <span>{t('content.hintRows')}</span>
        <span>
          <b className="font-bold text-accent">{t('content.hintOk')}</b> {t('content.hintOpen')}
        </span>
      </div>
    </div>
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
