// The single TanStack Query client for the SPA. It backs both route-loader
// prefetch (`queryClient.ensureQueryData`) and in-component reads
// (`useSuspenseQuery` / `useQuery`), so a value fetched by a loader is the same
// cache entry the component then reads no double fetch.
//
// Query functions run through the ad-hoc `kromaClient()` (see `queries.ts`), whose
// bearer is the in-memory session token; a 401 self-refreshes. So auth-gated
// queries just work once a session exists we gate *when* they run at the call
// site (only mount a suspense query once `ready && user`), matching the old
// `isAuthed()` loader guards.
import { QueryClient } from '@tanstack/react-query';

export function makeQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Catalogue data is stable within a visit; serve cache instantly on
        // back-nav and refetch in the background after this window.
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        // The client refreshes its own token on 401; a hard failure is usually a
        // real error (offline / gone), so don't hammer. Polling queries opt back
        // in per-call with `refetchInterval`.
        retry: 1,
        refetchOnWindowFocus: false,
      },
    },
  });
}

// One instance for the app's lifetime (SPA no per-request isolation needed).
export const queryClient = makeQueryClient();
