// Geometric spatial navigation over the DOM nodes react-native-web renders for
// our focusables. Web only: Apple TV and Android TV have an OS focus engine and
// never reach this code.
//
// Ported unchanged (behaviour-wise) from the pre-uikit @kroma/tv useFocusNav, so
// the existing Tizen / webOS feel is preserved exactly.

export type Direction = 'Up' | 'Down' | 'Left' | 'Right';

/** One candidate: the element and its rect, read ONCE per key press. On the
 * TV's weak CPU the dominant cost of a move is `getBoundingClientRect`, so a
 * 120-card grid must not read each rect twice (visibility + scoring). */
export interface Focusable {
  el: HTMLElement;
  rect: DOMRect;
}

/** A modal declares a focus SCOPE so the D-pad cannot wander back into the page
 * behind it. The LAST one in document order wins, which is the most recently
 * opened (React appends portals), so stacked dialogs behave correctly. The
 * native targets need no equivalent: their OS focus engine already confines
 * focus to the presented modal. */
function scope(): ParentNode {
  const scopes = document.querySelectorAll<HTMLElement>('[data-focus-scope]');
  return scopes[scopes.length - 1] ?? document;
}

export function focusables(): Focusable[] {
  const out: Focusable[] = [];
  for (const el of scope().querySelectorAll<HTMLElement>('[data-focus]')) {
    const rect = el.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) out.push({ el, rect });
  }
  return out;
}

/** Score a candidate at offset `dx,dy` for a move in `dir`, or `null` when it does
 * not lie in that direction. Lower is better; cross-axis drift is weighted x2 so we
 * prefer straight-line neighbours. */
export function directionScore(dir: Direction, dx: number, dy: number): number | null {
  switch (dir) {
    case 'Left':
      return dx >= -2 ? null : -dx + Math.abs(dy) * 2;
    case 'Right':
      return dx <= 2 ? null : dx + Math.abs(dy) * 2;
    case 'Up':
      return dy >= -2 ? null : -dy + Math.abs(dx) * 2;
    case 'Down':
      return dy <= 2 ? null : dy + Math.abs(dx) * 2;
  }
}

/** The Focusable currently holding focus, or the first candidate as a fallback. */
function current(els: Focusable[], first: Focusable): Focusable {
  const active = document.activeElement as HTMLElement | null;
  if (active?.dataset.focus === undefined) return first;
  return els.find((f) => f.el === active) ?? { el: active, rect: active.getBoundingClientRect() };
}

/** Move focus to the nearest focusable in `dir`. */
export function moveFocus(dir: Direction): void {
  const els = focusables();
  const first = els[0];
  if (!first) return; // nothing focusable on screen

  const from = current(els, first);
  const r = from.rect;
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;

  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const { el, rect: b } of els) {
    if (el === from.el) continue;
    const score = directionScore(dir, b.left + b.width / 2 - cx, b.top + b.height / 2 - cy);
    if (score != null && score < bestScore) {
      bestScore = score;
      best = el;
    }
  }

  if (best) {
    best.focus();
    best.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
  }
}

/** Focus the screen's entry point, unless a focusable already holds focus.
 * A `<Focusable autoFocus>` (tagged `data-autofocus`) wins; otherwise the first
 * focusable in DOM order, which is what the pre-uikit navigation always did. */
export function focusFirst(): void {
  const active = document.activeElement as HTMLElement | null;
  if (active && active.dataset?.focus !== undefined) return;
  const els = focusables();
  const target = els.find((f) => f.el.dataset.autofocus !== undefined) ?? els[0];
  target?.el.focus();
}

/** True when a real text field owns the keys (it needs its own left/right and
 * Backspace). */
export function inTextField(): boolean {
  const active = document.activeElement;
  return active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
}
