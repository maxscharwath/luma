// Thin web wrapper over the shared `useStoryboard` hook: injects the global
// `kromaClient()` so existing callers keep the `useStoryboard(itemId, opts?)`
// signature. All logic (lazy-generation polling, fast+slow backoff, visibility
// re-check, tile math) lives in `@kroma/ui`.

import { loadSession } from '@kroma/core';
import { useStoryboard as useSharedStoryboard } from '@kroma/ui';
import { useMemo } from 'react';
import { kromaClient } from '#web/shared/lib/api';

export type { Storyboard, StoryboardTile } from '@kroma/ui';

export function useStoryboard(itemId: string, opts?: { generate?: boolean }) {
  // `kromaClient()` mints a fresh KromaClient on every call, and the Player
  // re-renders constantly during playback. The shared hook keys its polling
  // effect on `client`, so an unstable reference would tear the effect down and
  // re-`poll()` on every render, cancelling the 1.5s/15s backoff timer and
  // hammering the storyboard endpoint while the sheet is still `pending`. Keep a
  // single client for the component's life, re-created only if the session token
  // changes.
  const token = loadSession()?.accessToken;
  // biome-ignore lint/correctness/useExhaustiveDependencies: token is an intentional trigger, not a referenced value. kromaClient() reads the session token internally, so we re-mint the client only when the token changes (per the comment above); dropping `token` would freeze the client on its first value.
  const client = useMemo(() => kromaClient(), [token]);
  return useSharedStoryboard(client, itemId, opts);
}
