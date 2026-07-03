// Shared 10-foot UI primitives for the redesigned TV app: brand mark, profile
// avatars, the radial auth backdrop, a wall clock, and the two remote-driven
// on-screen inputs (a full keyboard for server URLs / search, a numeric keypad
// for PINs). Everything interactive carries `data-focus` so the spatial focus
// nav (useFocusNav) reaches it and OK activates via the native click.
//
// Split by kind into sibling modules; this barrel keeps every export's name and
// the single `#tv/shared/ui` import path stable.

export { AuthScreen } from '#tv/shared/ui/AuthScreen';
export { AVATAR_GRADS, gradFor, initials, LockGlyph, ProfileAvatar } from '#tv/shared/ui/avatar';
export { TvBackButton } from '#tv/shared/ui/BackButton';
export { LumaMark, useClock } from '#tv/shared/ui/brand';
export { OnScreenKeyboard } from '#tv/shared/ui/keyboard';
export { Keypad } from '#tv/shared/ui/keypad';
export { artUrl, hostOf } from '#tv/shared/ui/util';
