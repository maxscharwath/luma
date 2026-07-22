// <Rail>: a titled horizontal row of tiles, the backbone of the 10-foot home.
//
// Scrolling follows focus on both sides without any per-rail wiring: the OS
// focus engine scrolls a ScrollView natively, and on the web the spatial
// navigator calls scrollIntoView after every move.

import type { ReactNode } from 'react';
import { ScrollView } from 'react-native';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { gutter } from '../tokens';

export interface RailProps {
  title?: string;
  /** Gap between tiles. */
  gap?: number;
  /** Side padding. Defaults to the overscan-safe 10-foot gutter, and it is
   *  applied INSIDE the scroller so the first tile's focus ring is never
   *  clipped by the viewport edge. */
  inset?: number;
  children: ReactNode;
}

export function Rail({ title, gap = 24, inset = gutter.tv, children }: Readonly<RailProps>) {
  return (
    <Box gap={16}>
      {title ? (
        <Txt variant="h2" style={{ paddingLeft: inset }}>
          {title}
        </Txt>
      ) : null}
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        // A focus ring is drawn OUTSIDE the tile's box, and a focused tile also
        // scales up, so the scroller needs vertical room or the ring is clipped.
        contentContainerStyle={{ gap, paddingHorizontal: inset, paddingVertical: 12 }}
      >
        {children}
      </ScrollView>
    </Box>
  );
}
