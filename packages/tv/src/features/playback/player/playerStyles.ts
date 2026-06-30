/** Shared 10-foot player control styling. Kept in one place so the control bar,
 * the up-next card and the skip-intro button all share the same focus ring +
 * pill chrome. Tailwind v4: no `/opacity` modifiers — use rgba() literals. */
export const FOCUS_RING = 'scale-[1.07] shadow-[var(--ring-focus),var(--glow-accent)]';
export const CTRL =
  'flex items-center justify-center rounded-full text-white transition-[transform,box-shadow,background] duration-180';
export const CTRL_ON = 'bg-[rgba(255,255,255,0.22)]';
export const CTRL_OFF = 'bg-[rgba(255,255,255,0.12)]';
export const PILL =
  'flex h-16 items-center gap-2.75 rounded-full px-7 font-sans text-[18px] font-bold text-white transition-[transform,box-shadow,background] duration-180';
