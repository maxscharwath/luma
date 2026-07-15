// Which registered module ids must be HIDDEN (nav, routes, panels), derived
// from the backend's module list. Pure so the rule is unit-testable: a module
// is visible only when the backend lists it AND it is enabled. With the
// zero-module base build, a compile-time-bundled UI (vpn, downloads, remote,
// ...) may have no installed backend at all; it must hide exactly like a
// disabled one (before, only an explicit `enabled: false` hid it, so
// uninstalled modules ghosted in the sidebar).

export interface ModuleListing {
  id: string;
  enabled?: boolean;
}

/** Registered ids to hide. `manifest === undefined` means the backend list has
 *  not resolved yet: hide nothing extra, so the nav doesn't flash out and back
 *  in on first load. */
export function hiddenModuleIds(
  manifest: readonly ModuleListing[] | undefined,
  registeredIds: readonly string[],
): ReadonlySet<string> {
  if (!manifest) return new Set<string>();
  const active = new Set(manifest.filter((m) => m.enabled !== false).map((m) => m.id));
  return new Set(registeredIds.filter((id) => !active.has(id)));
}
