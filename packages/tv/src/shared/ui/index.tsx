// The TV app's own 10-foot pieces: brand mark, profile avatars, the radial auth
// backdrop, a wall clock, and the two remote-driven on-screen inputs (a full
// keyboard for server URLs / search, a numeric keypad for PINs). Everything
// generic has moved to @kroma/ui/kit; what stays here is what depends on the TV
// app's own state (the device's keyboard-layout preference, the router).
//
// Split by kind into sibling modules; this barrel keeps every export's name and
// the single `#tv/shared/ui` import path stable.

export { AuthScreen } from '#tv/shared/ui/AuthScreen';
export { AVATAR_GRADS, gradFor, initials, LockGlyph, ProfileAvatar } from '#tv/shared/ui/avatar';
export { TvBackButton } from '#tv/shared/ui/BackButton';
export { KromaMark, useClock } from '#tv/shared/ui/brand';
export { OnScreenKeyboard } from '#tv/shared/ui/keyboard';
export { Keypad } from '#tv/shared/ui/keypad';
export { artUrl, hostOf } from '#tv/shared/ui/util';
