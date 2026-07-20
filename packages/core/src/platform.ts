// Which TV runtime is this? ONE spelling of each platform sniff, shared by every
// caller (codec probing in hevc/capabilities, the TV app's input environment and
// engine picker) so a shell is never identified two different ways.
//
// Each probe takes the user agent as an argument rather than reading `navigator`
// itself: callers disagree on what a missing `navigator` means (the codec probe
// treats it as "unknown browser", the TV input environment as "assume the TV"),
// so that decision stays with them.

/** Is `name` present on the global object? The TV platforms inject their bridge
 * objects there (`tizen`, `webOS`), which is a positive signal on its own. The
 * cast is read into a local first: this is a plain lookup on an untyped bag, not
 * a narrowing of `globalThis` itself. */
function hasGlobal(name: string): boolean {
  const globals = globalThis as Record<string, unknown>;
  return globals[name] !== undefined;
}

/** Samsung Tizen: the UA carries "Tizen" and the platform injects the `tizen`
 * web-API bridge (used by discover.ts / remote.ts). */
export function isTizenRuntime(ua: string): boolean {
  return /Tizen/i.test(ua) || hasGlobal('tizen');
}

/** LG webOS: firmwares disagree on the spelling, "Web0S" with a digit zero on
 * the TVs and "webOS" elsewhere, and the platform injects the `webOS` service
 * bridge (used by discover.ts). Both spellings and the bridge count. */
export function isWebOsRuntime(ua: string): boolean {
  return /Web0S|webOS/i.test(ua) || hasGlobal('webOS');
}
