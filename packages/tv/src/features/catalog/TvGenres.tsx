import {
  collectGenres,
  type GenreCount,
  genreAccent,
  genreColors,
  genreShowcases,
  genreTint,
  sizedImageUrl,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  Box,
  Focusable,
  fonts,
  gradient,
  Img,
  Txt,
  tintGradient,
  useFocusNav,
} from '@kroma/ui/kit';
import { useMemo } from 'react';
import { ScrollView } from 'react-native';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav } from '#tv/app/router';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';

// clamp(34px, 5.5vh, 60px) resolves to 59px on the fixed 1080-tall stage.
const TITLE = { fontSize: 59, lineHeight: 58, fontWeight: '700' as const, letterSpacing: -1.18 };

/** Genre picker: every genre in the library (movies + shows), most common first.
 * Selecting one drills into {@link TvGenreGrid}. Derives the genre list from the
 * already-loaded catalogue: no extra request, like {@link TvPerson}. Each card is
 * fronted by the genre's best-rated backdrop, washed in its signature colour. */
export function TvGenres() {
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  useFocusNav({ onBack: nav.back });

  const catalogue = useMemo(() => [...movies, ...shows], [movies, shows]);
  const genres = useMemo(() => collectGenres(catalogue), [catalogue]);
  const showcases = useMemo(() => genreShowcases(catalogue), [catalogue]);

  return (
    <Box fill bg="bg" overflow="hidden">
      <Box px={64} pt={112} pb={16}>
        <Txt variant="hero" style={TITLE}>
          {t('nav.genres')}
        </Txt>
      </Box>

      {genres.length ? (
        <ScrollView
          style={{ flex: 1, minHeight: 0 }}
          contentContainerStyle={{ paddingHorizontal: 64, paddingTop: 8, paddingBottom: 72 }}
          showsVerticalScrollIndicator={false}
        >
          <Box row wrap gap={12}>
            {genres.map((g) => {
              const pick = showcases.get(g.name);
              return (
                <GenreCard
                  key={g.name}
                  genre={g}
                  count={t('person.titleCount', { count: g.count })}
                  backdrop={pick ? client.backdropFor(pick) : null}
                  onPress={() => nav.go('genre', { name: g.name })}
                />
              );
            })}
          </Box>
        </ScrollView>
      ) : (
        <Box flex center px={64}>
          <Txt
            style={{ fontSize: 18, fontWeight: '500', textAlign: 'center', maxWidth: 640 }}
            color="textDim"
          >
            {t('genres.empty')}
          </Txt>
        </Box>
      )}

      {/* Persistent nav last in the tree so a genre tile keeps the initial focus. */}
      <TvTopNav active="genres" />
    </Box>
  );
}

/** One genre tile: library backdrop (or the genre-colour gradient) under a
 * bottom-heavy wash of the genre's hue. The tile's own padding keeps the amber
 * focus ring clear of the artwork. */
function GenreCard({
  genre,
  count,
  backdrop,
  onPress,
}: Readonly<{ genre: GenreCount; count: string; backdrop: string | null; onPress: () => void }>) {
  return (
    <Focusable onPress={onPress} label={genre.name} focusScale={1.04} style={CARD}>
      <Box aspect={16 / 9} radius={14} overflow="hidden" bg="surface1" shadow="card">
        <Img
          src={sizedImageUrl(backdrop, 328)}
          background={tintGradient(genreColors(genre.name))}
          position="50% 25%"
          fill
        />
        <Box fill pointerEvents="none" style={gradient(genreTint(genre.name))} />
        <Box absolute left={20} right={20} bottom={16} gap={2}>
          <Box h={4} w={28} radius="pill" bg={genreAccent(genre.name)} mb={8} />
          <Txt style={NAME}>{genre.name}</Txt>
          <Txt style={COUNT}>{count}</Txt>
        </Box>
      </Box>
    </Focusable>
  );
}

const CARD = { width: 340, flexShrink: 0, padding: 6, borderRadius: 20 } as const;
const NAME = {
  fontFamily: fonts.display,
  fontSize: 23,
  lineHeight: 24,
  fontWeight: '700' as const,
  color: '#FFFFFF',
};
const COUNT = {
  fontFamily: fonts.ui,
  fontSize: 14,
  fontWeight: '600' as const,
  color: 'rgba(255, 255, 255, 0.72)',
  fontVariant: ['tabular-nums' as const],
};
