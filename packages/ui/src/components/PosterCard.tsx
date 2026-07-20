import { memo } from 'react';
import { Image } from './Image';

export interface PosterCardProps {
  title: string;
  genre?: string;
  badge?: string | null;
  /** Two-stop gradient fallback when no poster image is available. */
  colors?: [string, string];
  /** Poster image URL (e.g. server-generated). Falls back to `colors` gradient. */
  poster?: string | null;
  progress?: number | null;
  width?: number;
  /** Focusable for TV remote navigation. */
  focusable?: boolean;
  onClick?: () => void;
}

/**
 * Poster tile used throughout KROMA rails and grids.
 *
 * Performance: the artwork is a real `<img loading="lazy" decoding="async">`, so
 * off-screen posters in long rails are never fetched or decoded until they
 * approach the viewport the key to staying smooth with hundreds of tiles
 * (Netflix/Disney+ style). The component is memoised so re-rendering a rail
 * doesn't re-render unaffected tiles.
 */
function PosterCardImpl({
  title,
  genre,
  badge = '4K',
  colors = ['#3A2E5C', '#0E1430'],
  poster = null,
  progress = null,
  width = 208,
  focusable = false,
  onClick,
}: Readonly<PosterCardProps>) {
  const gradient = `linear-gradient(158deg, ${colors[0]} 0%, ${colors[1]} 70%)`;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: when onClick is set the card carries role="button", tabIndex and an Enter/Space onKeyDown handler; the rule only fires because that role is applied conditionally.
    <div
      onClick={onClick}
      onKeyDown={(e) => {
        if (onClick && (e.key === 'Enter' || e.key === ' ')) onClick();
      }}
      tabIndex={focusable ? 0 : undefined}
      data-focus={focusable ? '' : undefined}
      role={onClick ? 'button' : undefined}
      className="kroma-poster"
      style={{ width, cursor: onClick ? 'pointer' : 'default', borderRadius: 'var(--radius-lg)' }}
    >
      <div
        style={{
          position: 'relative',
          aspectRatio: '2 / 3',
          borderRadius: 'var(--radius-lg)',
          overflow: 'hidden',
          background: gradient,
          boxShadow: 'var(--shadow-card)',
        }}
      >
        <Image src={poster} fit="cover" fill />
        <div
          style={{
            position: 'absolute',
            inset: 0,
            background: 'linear-gradient(170deg,rgba(0,0,0,.05) 35%,rgba(0,0,0,.72))',
          }}
        />
        {badge ? (
          <div
            style={{
              position: 'absolute',
              top: 10,
              right: 10,
              font: '700 10px var(--font-ui)',
              padding: '4px 7px',
              borderRadius: 5,
              background: 'rgba(10,10,12,.6)',
              color: 'var(--kroma-accent)',
            }}
          >
            {badge}
          </div>
        ) : null}
        <div style={{ position: 'absolute', left: 14, right: 14, bottom: 15 }}>
          {genre ? (
            <div
              style={{
                font: '700 10px var(--font-ui)',
                letterSpacing: '.12em',
                textTransform: 'uppercase',
                color: 'rgba(255,255,255,.6)',
                marginBottom: 5,
              }}
            >
              {genre}
            </div>
          ) : null}
          <div style={{ font: '700 20px var(--font-display)', color: '#fff' }}>{title}</div>
        </div>
        {progress != null ? (
          <div
            style={{
              position: 'absolute',
              left: 0,
              right: 0,
              bottom: 0,
              height: 5,
              background: 'rgba(255,255,255,.22)',
            }}
          >
            <div
              style={{ height: '100%', width: `${progress}%`, background: 'var(--kroma-accent)' }}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

export const PosterCard = memo(PosterCardImpl);
