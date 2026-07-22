// <PosterCard>: the 2:3 portrait tile of the browse grids (Films, Series).
//
// The same anatomy as <MediaCard> at a different aspect and type scale. It fills
// its grid cell rather than declaring a width, so the grid owns the column maths.

import { Focusable } from '../focus/Focusable';
import { gradient } from '../primitives/css';
import { Img } from '../primitives/Img';
import { Progress } from '../primitives/Progress';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { fonts, radius } from '../tokens';
import { tintGradient } from './MediaCard';
import { WatchedBadge } from './WatchedBadge';

/** Steeper than the rail tile's scrim: a poster is taller, so the fade has
 * further to travel before it reaches the title. */
export const POSTER_SCRIM =
  'linear-gradient(170deg, rgba(0, 0, 0, 0.05) 35%, rgba(0, 0, 0, 0.72) 100%)';

export interface PosterCardProps {
  title: string;
  /** Portrait key art. Falls back to the `tint` gradient. */
  art: string | null;
  tint: readonly [string, string];
  /** Resume / series-completion position, 0..1, or null for no bar. */
  progress?: number | null;
  watched?: boolean;
  onPress?: () => void;
  onFocus?: () => void;
  autoFocus?: boolean;
}

export function PosterCard({
  title,
  art,
  tint,
  progress = null,
  watched = false,
  onPress,
  onFocus,
  autoFocus,
}: Readonly<PosterCardProps>) {
  return (
    <Focusable
      onPress={onPress}
      onFocus={onFocus}
      autoFocus={autoFocus}
      label={title}
      focusScale={1.05}
      style={{ width: '100%', borderRadius: radius.lg }}
    >
      <Box aspect={2 / 3} radius="lg" overflow="hidden" bg="surface1" shadow="card">
        <Img src={art} background={tintGradient(tint)} fill />
        <Box fill style={gradient(POSTER_SCRIM)} />
        {watched ? <WatchedBadge size={26} /> : null}
        <Box absolute left={14} right={14} bottom={12}>
          <Txt style={TITLE} lines={2}>
            {title}
          </Txt>
        </Box>
        {progress == null ? null : (
          <Box absolute left={0} right={0} bottom={0}>
            <Progress value={progress} />
          </Box>
        )}
      </Box>
    </Focusable>
  );
}

const TITLE = {
  fontFamily: fonts.display,
  fontWeight: '700' as const,
  fontSize: 18,
  lineHeight: 19,
  color: '#FFFFFF',
};
