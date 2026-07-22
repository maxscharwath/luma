import { Box, Grid, PosterCard, useGrowingCount } from '@kroma/ui/kit';
import { memo } from 'react';
import { ScrollView } from 'react-native';

export interface GridCard {
  id: string;
  title: string;
  poster: string;
  colors: [string, string];
  /** Whether the current user has marked this title watched. */
  watched?: boolean;
  /** Series-completion / resume progress (%), or null. */
  progress?: number | null;
  onClick: () => void;
  /** Fired when the tile takes focus (drives the browse screens' ambient header). */
  onFocus?: () => void;
}

// Grid renders in chunks (grows on scroll) so a 1000-item library never mounts at once.
const GRID_STEP = 120;

// The 1920px stage makes the column maths static: 1792px of content is exactly
// 8 x 203px tiles plus 7 x 24px gaps. Flex wrap, never CSS grid, because the
// legacy webOS tier (Chromium 53) has no grid and React Native has none either.
const CONTENT_WIDTH = 1792;
const COLUMNS = 8;

/** Incrementally-rendered 2:3 poster grid for the Films / Séries browse views. */
function TvGridImpl({ cards }: Readonly<{ cards: GridCard[] }>) {
  const { count, onScroll, scrollEventThrottle } = useGrowingCount(cards.length, GRID_STEP);
  return (
    <ScrollView
      style={{ flex: 1, minHeight: 0 }}
      contentContainerStyle={{ paddingHorizontal: 64, paddingTop: 24, paddingBottom: 72 }}
      showsVerticalScrollIndicator={false}
      onScroll={onScroll}
      scrollEventThrottle={scrollEventThrottle}
    >
      <Grid width={CONTENT_WIDTH} columns={COLUMNS} gap={24} rowGap={32}>
        {cards.slice(0, count).map((c) => (
          <PosterCard
            key={c.id}
            title={c.title}
            art={c.poster}
            tint={c.colors}
            watched={c.watched}
            // GridCard carries a percentage (the server's series-completion
            // figure); <PosterCard> takes a 0..1 ratio.
            progress={c.progress == null ? null : c.progress / 100}
            onPress={c.onClick}
            onFocus={c.onFocus}
          />
        ))}
      </Grid>
      {count < cards.length ? <Box h={48} /> : null}
    </ScrollView>
  );
}

// memo: the browse screens re-render on every focus move (the ambient header
// tracks the focused tile); an unchanged `cards` array must skip this whole
// 100+-tile subtree.
export const TvGrid = memo(TvGridImpl);
