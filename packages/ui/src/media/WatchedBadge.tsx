// The filled amber check pinned to the top-left of a tile the user has watched.

import { Icon } from '../primitives/Icon';
import { Box } from '../system/Box';
import { colors } from '../tokens';

export interface WatchedBadgeProps {
  /** Disc diameter. 28 on a rail tile, 26 on a poster. */
  size?: number;
}

export function WatchedBadge({ size = 28 }: Readonly<WatchedBadgeProps>) {
  return (
    <Box
      absolute
      left={12}
      top={12}
      z={1}
      w={size}
      h={size}
      center
      radius="pill"
      bg="accent"
      shadow="card"
    >
      <Icon name="check" size={Math.round(size * 0.6)} color={colors.accentInk} stroke={3} />
    </Box>
  );
}
