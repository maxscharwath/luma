import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { ShowGrid } from '#web/features/catalog/cards';
import { isAuthed, lumaClient, toShowView } from '#web/shared/lib/api';

export const Route = createFileRoute('/series')({
  loader: async () => {
    if (!isAuthed()) return { shows: [] };
    const c = lumaClient();
    const shows = await c.shows();
    return { shows: shows.map((s) => toShowView(c, s)) };
  },
  component: SeriesPage,
});

function SeriesPage() {
  const t = useT();
  const { shows } = Route.useLoaderData();
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.series')}
      </h2>
      <ShowGrid shows={shows} />
    </main>
  );
}
