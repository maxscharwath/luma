// Thin web wrapper over the shared `useSubtitleGenerations` poll hook: injects the
// global `kromaClient()` and keeps the existing `(itemId, active, onComplete)`
// signature + `{ gens, cancel, refresh }` return shape for AvDrawer. All the poll /
// self-gating / seen-set logic lives in `@kroma/ui`.

import { useSubtitleGenerations as useSharedSubtitleGenerations } from '@kroma/ui';
import { kromaClient } from '#web/shared/lib/api';

export function useSubtitleGenerations(
  itemId: string,
  active: boolean,
  onComplete: (subId: string) => void,
) {
  const { generations, cancel, refresh } = useSharedSubtitleGenerations(kromaClient(), itemId, {
    active,
    onComplete,
  });
  return { gens: generations, cancel, refresh };
}
