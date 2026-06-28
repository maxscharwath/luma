import { dispatchRemoteKey, registerTvMediaKeys } from '@luma/core';
import { useEffect } from 'react';

function isVisible(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0;
}

function focusables(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-focus]')).filter(isVisible);
}

/** Geometric spatial navigation: move focus to the nearest element in `dir`. */
function moveFocus(dir: 'Up' | 'Down' | 'Left' | 'Right') {
  const els = focusables();
  if (els.length === 0) return;

  const active = document.activeElement as HTMLElement | null;
  const current = active && active.dataset.focus !== undefined ? active : els[0]!;
  const r = current.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;

  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const el of els) {
    if (el === current) continue;
    const b = el.getBoundingClientRect();
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
  useEffect(() => {
    registerTvMediaKeys();
    // Focus the first focusable on mount / view change.
    const first = focusables()[0];
    if (
      first &&
      (!document.activeElement ||
        (document.activeElement as HTMLElement).dataset?.focus === undefined)
    ) {
      first.focus();
    }

    const onKey = (e: KeyboardEvent) => {
      // When a text field is focused, let it own ◀ ▶ (cursor) and OK (submit);
      // only ▲ ▼ leave the field. Otherwise typing a server URL is impossible.
      const active = document.activeElement;
      const inText = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
      // Media keys keep their native default (no preventDefault) — handlers that
      // return `false` are treated as "not handled" by dispatchRemoteKey.
      const media = () => {
        onPlayPause?.();
        return false as const;
      };
      dispatchRemoteKey(
        e,
        {
          Back: () => onBack?.(),
          Play: media,
          Pause: media,
          PlayPause: media,
          Up: () => moveFocus('Up'),
          Down: () => moveFocus('Down'),
          Left: () => (inText ? false : moveFocus('Left')),
          Right: () => (inText ? false : moveFocus('Right')),
          Enter: () => {
            if (inText) return false; // native: submit the form / open the IME
            const el = active as HTMLElement | null;
            if (el?.dataset.focus === undefined) return false; // not on a focusable
            el.click(); // a fresh OK activates; auto-repeat is swallowed below
          },
        },
        // Ignore held OK so opening a new view (card → detail) can't carry over
        // and auto-activate the newly focused control (detail → player).
        { ignoreRepeat: ['Enter'] },
      );
    };

    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onBack, onPlayPause, resetKey]);
}
