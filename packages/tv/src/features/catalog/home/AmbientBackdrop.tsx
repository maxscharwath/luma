import { type KromaClient, type MediaItem, type Show, sizedImageUrl } from '@kroma/core';
import { Box, gradient, Img, tintGradient } from '@kroma/ui/kit';
import { useEffect, useState } from 'react';

/** `value`, but only after it has held still for `delayMs`. Lets a fast D-pad
 * sweep across a poster row settle before the full-screen art swaps, so the TV
 * never decodes a 1280px backdrop per focus step. */
function useSettled<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    if (value === settled) return;
    const id = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(id);
  }, [value, settled, delayMs]);
  return settled;
}

const SETTLE_MS = 350;
const FADE_MS = 500;

// Darkest bottom-left (title + grid zones), art shows through top-right: the
// Disney+ browse look. Two separate layers rather than one comma-separated
// background-image, because a multi-value background is a CSS-only luxury that
// React Native's gradient support does not have.
const VEIL_HORIZONTAL =
  'linear-gradient(90deg, rgba(10,10,12,0.8) 0%, rgba(10,10,12,0.38) 48%, rgba(10,10,12,0.12) 100%)';
const VEIL_VERTICAL =
  'linear-gradient(0deg, #0A0A0C 0%, rgba(10,10,12,0.78) 30%, rgba(10,10,12,0.35) 68%, rgba(10,10,12,0.12) 100%)';

/**
 * Full-screen ambient art for the browse screens: the focused title's backdrop,
 * debounced then cross-faded, dimmed by a veil so the poster grid stays legible.
 * Renders at `zIndex: -1` under the screen's own content.
 *
 * The cross-fade is <Img>'s own: it holds the previous art underneath until the
 * incoming one has decoded, then fades over it. That replaces the hand-rolled
 * two-layer stack this component used to carry, and it also fixes the bug that
 * stack existed to work around: the old fade was a CSS keyframe with `both`, and
 * an occluded window can skip animation frames entirely, leaving the layer stuck
 * invisible. A transition (web) and an Animated value (native) both settle on
 * their final value regardless of whether any frame was ever painted.
 */
export function AmbientBackdrop({
  src,
  colors,
}: Readonly<{ src: string | null; colors: [string, string] }>) {
  const settled = useSettled(src, SETTLE_MS);
  return (
    <Box fill z={-1} overflow="hidden" pointerEvents="none" accessibilityElementsHidden>
      <Img
        src={sizedImageUrl(settled, 1280)}
        background={tintGradient(colors)}
        position="50% 20%"
        duration={FADE_MS}
        fill
      />
      <Box fill pointerEvents="none" style={gradient(VEIL_HORIZONTAL)} />
      <Box fill pointerEvents="none" style={gradient(VEIL_VERTICAL)} />
    </Box>
  );
}

// ----- the art one catalogue entry contributes -------------------------------

/** One browse entry, a film or a series, with the fields the grids and the art
 * helpers below read. Shared by every screen that lists both kinds at once. */
export type CatalogEntry = { kind: 'movie'; item: MediaItem } | { kind: 'show'; item: Show };

/** The entry's poster (films and series resolve theirs from different endpoints). */
export function entryPoster(client: KromaClient, e: CatalogEntry): string {
  return e.kind === 'movie' ? client.posterFor(e.item) : client.showPosterFor(e.item);
}

/** The ambient art for the focused entry: its backdrop, falling back to its
 * poster, and nothing at all when the view is empty. One spelling of the chain
 * so every browse screen shows the same picture for the same title. */
export function entryBackdrop(client: KromaClient, e: CatalogEntry | null): string | null {
  if (!e) return null;
  return client.backdropFor(e.item) ?? entryPoster(client, e);
}
