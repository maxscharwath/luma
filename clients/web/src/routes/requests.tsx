import { createFileRoute } from '@tanstack/react-router';
import { MyRequestsPage } from '#web/features/requests/myRequests';

export const Route = createFileRoute('/requests')({
  component: MyRequestsPage,
});
