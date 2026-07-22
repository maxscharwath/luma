import {
  formatRuntime,
  posterColors,
  qualityBadgeForVideo,
  type ShowDetail,
  type UpNext,
} from '@kroma/core';
import { useLocale, useT, useThemeAudio } from '@kroma/ui';
import {
  Box,
  Button,
  Chip,
  Focusable,
  Icon,
  Img,
  Progress,
  Rail,
  Txt,
  tintGradient,
  useFocusNav,
  WatchedBadge,
} from '@kroma/ui/kit';
import { useEffect, useMemo, useState } from 'react';
import { useMyList } from '#tv/app/providers/mylist';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav, useParams } from '#tv/app/router';
import { TvDetailScaffold } from '#tv/features/catalog/detail/DetailScaffold';
import {
  CastRow,
  EndsAtHint,
  endsAtClock,
  ListButton,
  ThemeButton,
  WatchedButton,
} from '#tv/features/catalog/detail/parts';

export function TvShowDetail() {
  const nav = useNav();
  const { show } = useParams('show');
  const client = useClient();
  const t = useT();
  const locale = useLocale();
  const [detail, setDetail] = useState<ShowDetail | null>(null);
  const [season, setSeason] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const myList = useMyList();
  const watched = useWatched();

  // Per-episode resume progress (mapped by item id) for the episode thumbnails.
  const [epProgress, setEpProgress] = useState<Record<string, number>>({});
  // biome-ignore lint/correctness/useExhaustiveDependencies: show.id intentionally re-fetches when switching shows (the screen is reused on this route); it gates the effect even though the body reads it only indirectly.
  useEffect(() => {
    let cancelled = false;
    client
      .progress()
      .then((entries) => {
        if (cancelled) return;
        const map: Record<string, number> = {};
        for (const e of entries) {
          const dur = e.durationMs ?? 0;
          if (dur > 0 && e.positionMs > 0) {
            map[e.itemId] = Math.min(100, Math.round((e.positionMs / dur) * 100));
          }
        }
        setEpProgress(map);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
    // show.id: re-fetch when switching shows (the screen is reused on this route).
  }, [client, show.id]);

  useFocusNav({ onBack: nav.back, resetKey: detail });

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setSeason(null);
    setError(null);
    client
      .show(show.id)
      .then((d) => {
        if (cancelled) return;
        setDetail(d);
        setSeason(d.seasons[0]?.number ?? null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, show.id]);

  const meta = show.metadata;
  const backdrop = client.backdropFor(show) ?? client.showPosterFor(show);
  const theme = useThemeAudio(client.themeFor(show));

  const activeSeason = useMemo(
    () => detail?.seasons.find((s) => s.number === season) ?? detail?.seasons[0] ?? null,
    [detail, season],
  );
  const firstEpisode = activeSeason?.episodes[0] ?? null;

  // "Continue the series": resume in-progress, else next unwatched (per-user,
  // server-computed). Falls back to the first episode while loading.
  const [upNext, setUpNext] = useState<UpNext | null>(null);
  useEffect(() => {
    let cancelled = false;
    client
      .upNext(show.id)
      .then((r) => {
        if (!cancelled) setUpNext(r);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, show.id]);
  const playTarget = upNext?.item ?? firstEpisode;
  const playLabelKey = upNext?.resume ? 'player.resumeEpisode' : 'player.playEpisode';

  const metaLong = [
    show.year ? String(show.year) : null,
    t('content.seasonCount', { count: show.seasonCount }),
    t('content.episodeCount', { count: show.episodeCount }),
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <TvDetailScaffold
      id={show.id}
      kind={t('content.series')}
      title={show.title}
      backdrop={backdrop}
      rating={meta?.rating}
      meta={metaLong}
      badge={qualityBadgeForVideo(show.video)}
      overview={meta?.overview}
    >
      <Box row align="center" gap={16}>
        <Button
          size="lg"
          icon="player-play-filled"
          disabled={!playTarget}
          label={
            playTarget
              ? t(playLabelKey, {
                  season: playTarget.season ?? 0,
                  episode: playTarget.episode ?? 0,
                })
              : t('player.play')
          }
          onPress={() => playTarget && nav.go('player', { item: playTarget })}
        />
        <ListButton inList={myList.has(show.id)} onToggle={() => myList.toggle(show.id)} />
        <WatchedButton watched={watched.has(show.id)} onToggle={() => watched.toggle(show.id)} />
        {theme.active ? <ThemeButton muted={theme.muted} onToggle={theme.toggle} /> : null}
      </Box>
      {/* Match the Play button's target (resume/next episode), not always ep 1. */}
      <EndsAtHint runtimeMs={playTarget?.durationMs} />

      {error ? (
        <Txt variant="title" color="textMuted" style={STATUS}>
          {t('content.loadEpisodesFailed', { error })}
        </Txt>
      ) : null}
      {!detail && !error ? (
        <Txt variant="title" color="textMuted" style={STATUS}>
          {t('content.loadingEpisodes')}
        </Txt>
      ) : null}

      {detail && detail.seasons.length > 1 ? (
        <Box row align="center" gap={18} mt={30}>
          <Txt style={SEASON_LABEL} color="textMuted">
            {t('content.seasonsHeader')}
          </Txt>
          <Rail inset={12} gap={10}>
            {detail.seasons.map((s) => (
              <Chip
                key={s.number}
                variant="surface"
                focusScale={1.05}
                active={s.number === activeSeason?.number}
                label={t('content.season', { number: s.number })}
                onPress={() => setSeason(s.number)}
                style={SEASON_CHIP}
              />
            ))}
          </Rail>
        </Box>
      ) : null}

      {/* Cast for the selected season (TMDB season credits), falling back to the
          show's overall cast until the season is enriched. */}
      <CastRow cast={activeSeason?.cast?.length ? activeSeason.cast : meta?.cast} />

      {activeSeason ? (
        <Box mt={32} gap={16}>
          <Txt style={EPISODES_LABEL} color="rgba(244, 243, 240, 0.55)">
            {t('content.episodesHeader')}
          </Txt>
          <Rail inset={0} gap={18}>
            {activeSeason.episodes.map((ep) => (
              // The focus ring belongs to the thumbnail only (design) title +
              // meta sit below it, outside the amber border.
              <Box key={ep.id} w={260} shrink={0} gap={9}>
                {/* The focus ring belongs to the thumbnail only (design): title
                    and meta sit below it, outside the amber border. */}
                <Focusable
                  onPress={() => nav.go('player', { item: ep })}
                  label={ep.episodeTitle ?? ep.title}
                  style={{ borderRadius: 12 }}
                >
                  <Box aspect={16 / 9} center radius={12} overflow="hidden" bg="surface1">
                    <Img
                      src={client.backdropFor(ep) ?? backdrop}
                      background={tintGradient(posterColors(ep.id))}
                      position="50% 30%"
                      fill
                    />
                    {watched.has(ep.id) ? <WatchedBadge size={26} /> : null}
                    <Box w={46} h={46} center radius="pill" bg="rgba(10, 10, 12, 0.5)">
                      <Icon name="player-play-filled" size={18} color="#FFFFFF" />
                    </Box>
                    {epProgress[ep.id] != null && !watched.has(ep.id) ? (
                      <Box absolute left={0} right={0} bottom={0}>
                        <Progress value={(epProgress[ep.id] ?? 0) / 100} />
                      </Box>
                    ) : null}
                  </Box>
                </Focusable>
                <Txt style={EPISODE_TITLE}>{`${ep.episode}. ${ep.episodeTitle ?? ep.title}`}</Txt>
                <Box row align="center" gap={8}>
                  <Txt style={EPISODE_META} color="textDim">
                    {formatRuntime(ep.durationMs)}
                  </Txt>
                  {endsAtClock(ep.durationMs, locale) ? (
                    <>
                      <Txt style={[EPISODE_META, { opacity: 0.4 }]} color="textDim">
                        ·
                      </Txt>
                      <Box row align="center" gap={6}>
                        <Icon name="clock" size={12} stroke={2} color="accent" />
                        <Txt style={EPISODE_META} color="textDim">
                          {t('content.endsAtShort', { time: endsAtClock(ep.durationMs, locale) })}
                        </Txt>
                      </Box>
                    </>
                  ) : null}
                </Box>
              </Box>
            ))}
          </Rail>
        </Box>
      ) : null}
    </TvDetailScaffold>
  );
}

const STATUS = { marginTop: 24, fontWeight: '400' as const };
const SEASON_LABEL = { fontSize: 15, fontWeight: '700' as const, letterSpacing: 0.6 };
const SEASON_CHIP = { paddingVertical: 9, paddingHorizontal: 20, borderWidth: 0 } as const;
const EPISODES_LABEL = {
  fontSize: 15,
  fontWeight: '700' as const,
  letterSpacing: 0.6,
  textTransform: 'uppercase' as const,
};
const EPISODE_TITLE = { fontSize: 15, fontWeight: '600' as const };
const EPISODE_META = {
  fontSize: 13,
  fontWeight: '500' as const,
  fontVariant: ['tabular-nums' as const],
};
