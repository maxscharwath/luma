// Responsive poster grid. One source of truth for the column math: the grid
// derives columns from breakpoints and sizes cards to fill the row exactly, so
// no gutter is left over on the right.

import { FlatList, useWindowDimensions } from 'react-native';
import { spacing, TAB_BAR_CLEARANCE } from '../lib/theme';
import { type CardModel, PosterCard } from './cards';

const GAP = 12;

export function gridMetrics(width: number): { cols: number; cardW: number } {
  let cols = 3;
  if (width >= 1000) cols = 6;
  else if (width >= 700) cols = 5;
  else if (width >= 500) cols = 4;
  const cardW = Math.floor((width - spacing.md * 2 - GAP * (cols - 1)) / cols);
  return { cols, cardW };
}

export function PosterGrid({
  cards,
  header,
  empty,
  refreshing,
  onRefresh,
}: Readonly<{
  cards: CardModel[];
  header?: React.ReactElement;
  /** Centered placeholder when there is nothing to show. */
  empty?: React.ReactElement;
  refreshing?: boolean;
  onRefresh?: () => void;
}>) {
  const { width } = useWindowDimensions();
  const { cols, cardW } = gridMetrics(width);
  return (
    <FlatList
      key={cols}
      data={cards}
      numColumns={cols}
      keyExtractor={(c) => c.key}
      renderItem={({ item }) => <PosterCard card={item} width={cardW} />}
      columnWrapperStyle={cols > 1 ? { gap: GAP } : undefined}
      contentContainerStyle={{
        paddingHorizontal: spacing.md,
        paddingBottom: TAB_BAR_CLEARANCE,
        gap: spacing.md,
        flexGrow: cards.length === 0 ? 1 : undefined,
      }}
      ListHeaderComponent={header}
      ListEmptyComponent={empty}
      refreshing={refreshing}
      onRefresh={onRefresh}
      initialNumToRender={12}
      removeClippedSubviews
    />
  );
}
