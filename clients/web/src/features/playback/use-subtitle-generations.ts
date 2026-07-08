// Thin web wrapper over the shared `useSubtitleGenerations` poll hook: injects the
// global `lumaClient()` and keeps the existing `(itemId, active, onComplete)`
// signature + `{ gens, cancel, refresh }` return shape for AvDrawer. All the poll /
// self-gating / seen-set logic lives in `@luma/ui`.

import { useSubtitleGenerations as useSharedSubtitleGenerations } from '@luma/ui';
import { lumaClient } from '#web/shared/lib/api';

export function useSubtitleGenerations(
  itemId: string,
  active: boolean,
  onComplete: (subId: string) => void,
) {
  const { generations, cancel, refresh } = useSharedSubtitleGenerations(lumaClient(), itemId, {
    active,
    onComplete,
  });
  return { gens: generations, cancel, refresh };
}
