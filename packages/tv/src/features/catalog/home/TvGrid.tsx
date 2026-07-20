import { memo } from 'react';
import { useGrowingCount } from '#tv/features/catalog/home/useGrowingCount';
import { TvPoster } from '#tv/shared/TvMedia';

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

/** Incrementally-rendered 2:3 poster grid for the Films / Séries browse views. */
function TvGridImpl({ cards }: Readonly<{ cards: GridCard[] }>) {
  const [count, sentinel] = useGrowingCount(cards.length, GRID_STEP);
  return (
    <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-16 pt-6 pb-18">
      {/* flex-wrap, NOT CSS grid: the fixed 1920px stage makes the column math
          static (1792px content = 8 x 203px + 7 x 24px gaps), and flex survives
          the legacy webOS tier (Chromium 53) where grid does not exist. */}
      <div className="flex flex-wrap gap-x-6 gap-y-8">
        {cards.slice(0, count).map((c) => (
          <div key={c.id} className="w-[203px]">
            <TvPoster
              title={c.title}
              poster={c.poster}
              colors={c.colors}
              watched={c.watched}
              progress={c.progress}
              onClick={c.onClick}
              onFocus={c.onFocus}
            />
          </div>
        ))}
      </div>
      {count < cards.length ? <div ref={sentinel} className="h-12 w-full" /> : null}
    </div>
  );
}

// memo: the browse screens re-render on every focus move (the ambient header
// tracks the focused tile); an unchanged `cards` array must skip this whole
// 100+-tile subtree.
export const TvGrid = memo(TvGridImpl);
