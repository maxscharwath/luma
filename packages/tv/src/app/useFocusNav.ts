import { dispatchRemoteKey, registerTvMediaKeys } from '@kroma/core';
import { useEffect } from 'react';
import { useEnv } from '#tv/app/providers/env';

// Don't let one physical OK carry past the screen it opened. The press that
// navigates here (card → detail) would otherwise also fire the control the new
// screen auto-focuses (detail → Play → player), via the remote's key repeat or a
// keyup/keydown bounce. So we ignore OK for a short window after every screen
// mounts (see the effect) long enough to swallow the stray repeat, short enough
// that a deliberate second press still lands. Module-scope so it survives the
// transition's unmount/mount.
let okGuardUntil = 0;

/** One candidate: the element and its rect, read ONCE per key press. On the
 * TV's weak CPU the dominant cost of a move is `getBoundingClientRect`, so a
 * 120-card grid must not read each rect twice (visibility + scoring). */
interface Focusable {
  el: HTMLElement;
  rect: DOMRect;
}

function focusables(): Focusable[] {
  const out: Focusable[] = [];
  for (const el of document.querySelectorAll<HTMLElement>('[data-focus]')) {
    const rect = el.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) out.push({ el, rect });
  }
  return out;
}

/** Geometric spatial navigation: move focus to the nearest element in `dir`. */
function moveFocus(dir: 'Up' | 'Down' | 'Left' | 'Right') {
  const els = focusables();
  const first = els[0];
  if (!first) return; // nothing focusable on screen

  const active = document.activeElement as HTMLElement | null;
  const activeFocusable = active && active.dataset.focus !== undefined;
  const current: Focusable =
    (activeFocusable && els.find((f) => f.el === active)) ||
    (activeFocusable && active ? { el: active, rect: active.getBoundingClientRect() } : first);
  const r = current.rect;
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;

  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const { el, rect: b } of els) {
    if (el === current.el) continue;
    const bx = b.left + b.width / 2;
    const by = b.top + b.height / 2;
    const dx = bx - cx;
    const dy = by - cy;

    let primary: number;
    let secondary: number;
    switch (dir) {
      case 'Left':
        if (dx >= -2) continue;
        primary = -dx;
        secondary = Math.abs(dy);
        break;
      case 'Right':
        if (dx <= 2) continue;
        primary = dx;
        secondary = Math.abs(dy);
        break;
      case 'Up':
        if (dy >= -2) continue;
        primary = -dy;
        secondary = Math.abs(dx);
        break;
      case 'Down':
        if (dy <= 2) continue;
        primary = dy;
        secondary = Math.abs(dx);
        break;
    }
    // Weight cross-axis drift heavily so we prefer straight-line neighbours.
    const score = primary + secondary * 2;
    if (score < bestScore) {
      bestScore = score;
      best = el;
    }
  }

  if (best) {
    best.focus();
    best.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
  }
}

export interface FocusNavHandlers {
  onBack?: () => void;
  onPlayPause?: () => void;
  /** Re-run when this changes (e.g. view switch) to focus the first element. */
  resetKey?: unknown;
}

/**
 * Wires TV remote / keyboard input to spatial focus movement across any element
 * carrying a `data-focus` attribute (e.g. `<PosterCard focusable />`).
 */
export function useFocusNav({ onBack, onPlayPause, resetKey }: FocusNavHandlers) {
  const { pointer } = useEnv();
  // biome-ignore lint/correctness/useExhaustiveDependencies: resetKey is an intentional re-run trigger (a view switch re-focuses the first element); it is not read inside the effect.
  useEffect(() => {
    registerTvMediaKeys();
    // Arm the OK guard before the keydown listener attaches, so the press that
    // navigated here can't beat it and activate the control we auto-focus below.
    okGuardUntil = Date.now() + 300;
    // Focus the first focusable on mount / view change.
    const first = focusables()[0];
    if (
      first &&
      (!document.activeElement ||
        (document.activeElement as HTMLElement).dataset?.focus === undefined)
    ) {
      first.el.focus();
    }

    const onKey = (e: KeyboardEvent) => {
      // When a text field is focused, let it own ◀ ▶ (cursor) and OK (submit);
      // only ▲ ▼ leave the field. Otherwise typing a server URL is impossible.
      const active = document.activeElement;
      const inText = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
      // Media keys keep their native default (no preventDefault) handlers that
      // return `false` are treated as "not handled" by dispatchRemoteKey.
      const media = () => {
        onPlayPause?.();
        return false as const;
      };
      dispatchRemoteKey(
        e,
        {
          Back: (ev) => {
            // In a real text field a physical Backspace edits the value (native);
            // only Escape / a remote Back button leaves the screen.
            if (inText && ev.key === 'Backspace') return false;
            return onBack?.();
          },
          Play: media,
          Pause: media,
          PlayPause: media,
          Up: () => moveFocus('Up'),
          Down: () => moveFocus('Down'),
          Left: () => (inText ? false : moveFocus('Left')),
          Right: () => (inText ? false : moveFocus('Right')),
          Enter: () => {
            if (inText) return false; // native: submit the form / open the IME
            if (Date.now() < okGuardUntil) return; // tail of the press that opened this screen
            const el = active as HTMLElement | null;
            if (el?.dataset.focus === undefined) return false; // not on a focusable
            el.click();
          },
        },
        // Held OK auto-repeats are swallowed too (the mount guard above only spans
        // the first instant after a transition).
        { ignoreRepeat: ['Enter'] },
      );
    };

    window.addEventListener('keydown', onKey);

    // Desktop mouse / webOS magic remote: let the amber focus ring follow the
    // cursor, so a pointer drives the same `[data-focus]:focus` affordance the
    // remote does (no visual/design change, `preventScroll` keeps hover from
    // yanking rails the way remote nav intentionally does). Clicks already work
    // natively everything focusable is a real button / role="button".
    const onPointerOver = pointer
      ? (e: PointerEvent) => {
          const active = document.activeElement;
          // Never yank focus out of a field the user may be typing into (mouse
          // jitter over the on-screen keyboard mustn't blur a half-typed query).
          if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) return;
          const el = (e.target as HTMLElement | null)?.closest<HTMLElement>('[data-focus]');
          if (el && el !== active) el.focus({ preventScroll: true });
        }
      : null;
    if (onPointerOver) window.addEventListener('pointerover', onPointerOver);

    return () => {
      window.removeEventListener('keydown', onKey);
      if (onPointerOver) window.removeEventListener('pointerover', onPointerOver);
    };
  }, [onBack, onPlayPause, resetKey, pointer]);
}
