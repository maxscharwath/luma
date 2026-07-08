import { createFileRoute } from '@tanstack/react-router';
import { MyRequestsPage } from '#web/features/requests/my-requests';

export const Route = createFileRoute('/requests')({
  component: MyRequestsPage,
});
