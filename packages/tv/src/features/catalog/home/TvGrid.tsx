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
}

// Grid renders in chunks (grows on scroll) so a 1000-item library never mounts at once.
const GRID_STEP = 120;

/** Incrementally-rendered 2:3 poster grid for the Films / Séries browse views. */
export function TvGrid({ cards }: Readonly<{ cards: GridCard[] }>) {
  const [count, sentinel] = useGrowingCount(cards.length, GRID_STEP);
  return (
    <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-16 pt-7 pb-18">
      <div className="grid grid-cols-[repeat(auto-fill,minmax(188px,1fr))] gap-x-6 gap-y-8">
        {cards.slice(0, count).map((c) => (
          <TvPoster
            key={c.id}
            title={c.title}
            poster={c.poster}
            colors={c.colors}
            watched={c.watched}
            progress={c.progress}
            onClick={c.onClick}
          />
        ))}
      </div>
      {count < cards.length ? <div ref={sentinel} className="h-12 w-full" /> : null}
    </div>
  );
}
