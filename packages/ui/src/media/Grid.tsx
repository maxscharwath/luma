// <Grid>: the fixed-column tile grid of the browse screens.
//
// React Native has no CSS grid (and neither does the legacy webOS tier this app
// still ships to), so the columns are computed and the children laid out with
// flex wrap. Each cell gets an explicit width, which is also what lets a
// <PosterCard> simply fill its cell.

import { Children, type ReactNode } from 'react';
import { Box } from '../system/Box';

export interface GridProps {
  /** Total width available to the grid, gutters included. */
  width: number;
  columns: number;
  /** Horizontal gap, which is also what the column maths removes. */
  gap?: number;
  /** Vertical gap. Defaults to `gap`; the browse grids run looser vertically so
   *  the rows read as rows rather than as one field of tiles. */
  rowGap?: number;
  children: ReactNode;
}

/** The width of one cell in a `columns`-wide grid of `width`, with `gap`
 * between cells. Exported so a caller can size its own art requests to match. */
export function cellWidth(width: number, columns: number, gap: number): number {
  if (columns <= 0) return width;
  return Math.floor((width - gap * (columns - 1)) / columns);
}

export function Grid({ width, columns, gap = 24, rowGap, children }: Readonly<GridProps>) {
  const cell = cellWidth(width, columns, gap);
  return (
    <Box row wrap gap={gap} style={{ rowGap: rowGap ?? gap }}>
      {Children.map(children, (child) => (
        <Box w={cell}>{child}</Box>
      ))}
    </Box>
  );
}
