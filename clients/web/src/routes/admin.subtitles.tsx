import { createFileRoute } from '@tanstack/react-router';
import { SubtitlesPage } from '#web/features/admin/subtitlesAdmin';

export const Route = createFileRoute('/admin/subtitles')({
  component: SubtitlesPage,
});
