// Thin TV wrapper over the shared `useStoryboard` hook: keeps the existing
// `useStoryboard(client, itemId)` call site. All logic (lazy-generation polling,
// fast+slow backoff, visibility re-check, tile math) lives in `@kroma/ui`.

import type { KromaClient } from '@kroma/core';
import { useStoryboard as useSharedStoryboard } from '@kroma/ui';

export type { Storyboard, StoryboardTile } from '@kroma/ui';

export function useStoryboard(client: KromaClient, itemId: string) {
  return useSharedStoryboard(client, itemId);
}
