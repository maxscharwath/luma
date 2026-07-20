import { type KromaClient, type MediaItem, type Show, sizedImageUrl } from '@kroma/core';
import { useEffect, useState } from 'react';
import { TvArt } from '#tv/shared/TvMedia';

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

// The three coupled timings of a swap, in order:
//   focus settles (SETTLE_MS) → the new layer fades in (FADE_MS) → the outgoing
//   layer is dropped (COLLAPSE_MS).
// INVARIANT: COLLAPSE_MS > FADE_MS, or the art underneath is unmounted while the
// incoming layer is still translucent (a flash of the bare background).
const SETTLE_MS = 350;
const FADE_MS = 500;
// A margin over the fade so a TV that started the animation a frame or two late
// still finishes it before the layer underneath goes away.
const COLLAPSE_MS = FADE_MS + 200;

/** The cross-fade class. Its 0.5s duration is spelled out literally because
 * Tailwind only emits classes it can see verbatim in the source, so keep it
 * equal to FADE_MS (the `tv-ambient-in` keyframe lives in tv.css). */
const FADE_IN = 'animate-[tv-ambient-in_0.5s_ease_both]';

interface AmbientLayer {
  src: string | null;
  colors: [string, string];
  /** This layer is cross-fading in. Carried ON THE LAYER, never derived from the
   * render index: a third swap shifts a still-fading layer down the array, and
   * stripping its animation class mid-fade would snap it to full opacity. */
  enter?: boolean;
}

/** Keep only the incoming layer and end its fade (the outgoing art is no longer
 * showing through, and a finished animation must not keep its class: an occluded
 * window may never have run it, and `both` would leave the layer invisible).
 * Returns `prev` untouched when there is nothing to do, so React can skip the
 * re-render. */
function collapse(prev: AmbientLayer[]): AmbientLayer[] {
  const last = prev.at(-1);
  if (!last) return prev;
  if (prev.length === 1 && !last.enter) return prev;
  return [last.enter ? { ...last, enter: false } : last];
}

// Darkest bottom-left (title + grid zones), art shows through top-right the
// Disney+ browse look. rgba() literals for the legacy webOS tier.
const VEIL =
  'pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(10,10,12,0.8)_0%,rgba(10,10,12,0.38)_48%,rgba(10,10,12,0.12)_100%),linear-gradient(0deg,#0A0A0C_0%,rgba(10,10,12,0.78)_30%,rgba(10,10,12,0.35)_68%,rgba(10,10,12,0.12)_100%)]';

/**
 * Full-screen ambient art for the browse screens: the focused title's backdrop,
 * debounced then cross-faded (the previous layer stays mounted under the new one
 * until its fade completes), dimmed by a veil so the poster grid stays legible.
 * Renders at `-z-1` under the screen's own content the parent must `isolate`.
 */
export function AmbientBackdrop({
  src,
  colors,
}: Readonly<{ src: string | null; colors: [string, string] }>) {
  const settled = useSettled(src, SETTLE_MS);
  const [layers, setLayers] = useState<AmbientLayer[]>(() => [{ src: settled, colors }]);

  // Push a cross-fade layer when the settled art changes, keeping at most two
  // (outgoing + incoming) so a long browse session never stacks decodes. The
  // collapse to one layer rides a timer, NOT animationend: a throttled/occluded
  // window can suppress animation frames entirely, and the timer then still
  // drops the animated class (clearing `enter`) so the incoming art snaps visible.
  // biome-ignore lint/correctness/useExhaustiveDependencies: colors is read as a snapshot when the settled src changes (it is the matching fallback gradient), not a trigger of its own.
  useEffect(() => {
    setLayers((prev) =>
      prev.at(-1)?.src === settled
        ? prev
        : [...prev.slice(-1), { src: settled, colors, enter: true }],
    );
    const id = setTimeout(() => setLayers(collapse), COLLAPSE_MS);
    return () => clearTimeout(id);
  }, [settled]);

  return (
    <div aria-hidden className="absolute inset-0 -z-1 overflow-hidden">
      {layers.map((l) => (
        <div key={l.src ?? 'gradient'} className={`absolute inset-0 ${l.enter ? FADE_IN : ''}`}>
          <TvArt src={sizedImageUrl(l.src, 1280)} colors={l.colors} position="50% 20%" />
        </div>
      ))}
      <div className={VEIL} />
    </div>
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
