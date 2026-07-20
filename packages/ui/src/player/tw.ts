/**
 * Shared Tailwind class-string constants for the unified player. Now that the
 * web + TV Tailwind builds scan `packages/ui/src/player` (see each client's
 * `@source`), the chrome is styled with Tailwind like the rest of the codebase.
 *
 * LEGACY TIER RULES (old webOS, Chromium 53, see tv-build/check-legacy): stay
 * flex-only (no CSS grid), and NEVER use `/opacity` modifiers - use rgba() via
 * arbitrary values (`bg-[rgba(255,255,255,0.12)]`) or the semantic token classes
 * (accent / accent-ink / text / dim / surface-*). Focus is STATE-driven (hover
 * moves focus exactly like the D-pad), so we toggle these classes on a boolean,
 * never via CSS :hover / :focus.
 */

/** The unified amber focus ring + spring pop for any focused control. */
export const FOCUS_RING = 'scale-[1.07] shadow-[var(--ring-focus),var(--glow-accent)]';
/** Thinner ring for dense rows (settings entries, cards). */
export const FOCUS_RING_SM = 'shadow-[var(--ring-focus-sm),var(--glow-accent)]';

/** Circular transport / cluster control base. */
export const CTRL =
  'flex flex-none items-center justify-center rounded-full text-white outline-none border-none cursor-pointer transition-[transform,box-shadow,background] duration-150 ease-out';
export const CTRL_ON = 'bg-[rgba(255,255,255,0.22)]';
export const CTRL_OFF = 'bg-[rgba(255,255,255,0.12)]';

/** A translucent capsule (e.g. the volume pill container). */
export const PILL_WRAP =
  'flex flex-none items-center rounded-full overflow-hidden transition-[transform,box-shadow,background] duration-150 ease-out';

/** Section eyebrow (uppercase, tracked, dim). */
export const EYEBROW =
  'font-sans text-[12px] font-bold uppercase tracking-[0.14em] text-[rgba(244,243,240,0.45)]';

/** A right-side sliding panel surface (settings / AV drawer). */
export const PANEL =
  'absolute inset-y-0 right-0 z-42 flex flex-col overflow-y-auto bg-[rgba(16,16,20,0.94)] backdrop-blur-2xl animate-[kpl-panel-in_0.28s_var(--ease-out)_both]';
