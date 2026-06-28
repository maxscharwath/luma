import { useGrowingCount } from '#tv/home/useGrowingCount';
import { TvPoster } from '#tv/TvMedia';

export interface GridCard {
  id: string;
  title: string;
  badge: string | null;
  poster: string;
  colors: [string, string];
  onClick: () => void;
}

// Grid renders in chunks (grows on scroll) so a 1000-item library never mounts at once.
const GRID_STEP = 120;

/** Incrementally-rendered 2:3 poster grid for the Films / Séries browse views. */
export function TvGrid({ label, cards }: Readonly<{ label: string; cards: GridCard[] }>) {
  const [count, sentinel] = useGrowingCount(cards.length, GRID_STEP);
  return (
    <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-16 pt-6 pb-18">
      <div className="mb-5 font-sans text-[15px] font-bold tracking-[0.04em] text-muted">
        {label} · {cards.length}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(188px,1fr))] gap-x-6 gap-y-8">
        {cards.slice(0, count).map((c) => (
          <TvPoster
            key={c.id}
            title={c.title}
            badge={c.badge}
            poster={c.poster}
            colors={c.colors}
            onClick={c.onClick}
          />
        ))}
      </div>
      {count < cards.length ? <div ref={sentinel} className="h-12 w-full" /> : null}
    </div>
  );
}
