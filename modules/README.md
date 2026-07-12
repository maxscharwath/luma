# Authoring a LUMA module

LUMA is a modular player: the core is playback + catalog, and everything else
(downloads, indexers, requests, dashboards, ...) is a **module**. A module has a
stable **reverse-DNS id** (e.g. `dev.luma.notes`) that joins its backend and
frontend halves, a `module.json` manifest, and it registers itself with zero
hand-wiring. Pick the shape that fits, then use the commands below.

## Which shape?

| Shape | Use it for | Where | Runtime install? |
|-------|-----------|-------|------------------|
| **Single-file** (codegen) | most modules: a manifest + a page (+ optional backend items) | `modules/<name>.module.md` | no (compiled in) |
| **Crate** (hand-written) | a domain module with a substantial backend and a bundled page | `server/modules/<id>/{be,fe}` | no (compiled in) |
| **WASM** (runtime) | a module you install into a running server with no rebuild | `wasm-modules/<id>/{be,fe}` | **yes** |

Start with **single-file** unless you specifically need the other two.

## 1. Single-file module (the default)

```
bun run modules:new dev.luma.notes     # scaffold modules/notes.module.md
# edit the file: YAML frontmatter (manifest) + a ```tsx page (+ optional ```svg / ```rust / ```sql)
bun run modules:gen                     # expand it into server/modules/<id>/ + register it
bun run modules:validate                # schema-check every manifest
```

One `.module.md` holds the manifest (frontmatter) and its fenced blocks:
`` ```tsx `` (the page, required), `` ```svg `` (packaged icon), `` ```rust ``
(extra backend items -- the registry entry `pub const MODULE` is generated for
you; do not redefine it), `` ```sql `` (migrations). `modules:gen` writes the
generated crate + package and updates the aggregator rosters, so nothing is
hand-wired. See `modules/hello.module.md` for a complete example. Generated
output is committed -- re-run `modules:gen` after editing and commit the result.

## 2. Hand-written crate module

Look at `server/modules/dev.luma.torrents/` (backend + frontend) as the template. Each
crate exports one `pub const MODULE` built from its `module.json` + `icon.svg`:

```rust
pub const MODULE: EmbeddedModule =
    EmbeddedModule::new(include_str!("../../module.json"), include_bytes!("../../icon.svg"));
```

Register the backend by adding the module to `modules/roster.yaml` (its `id` +
`crate`, plus `serverModule: true` when it ships one) and running
`bun run modules:gen`, which regenerates the `luma-modules-generated` aggregator.
For a compiled-in frontend, add it to `clients/web/src/modules/registry.ts`. A
module that also owns admin routes + start/stop lifecycle implements
`ServerModule` in its OWN `server/` crate (see `dev.luma.torrents`).

## 3. WASM runtime module (install with no rebuild)

Look at `wasm-modules/dev.luma.hellowasm/`: a `server/` extism guest (exports
`handle_http`, proxied at `/api/plugin/<id>/*`), a `ui/` Module Federation remote
(the page), `module.json` (with `feRemote`), and `icon.svg`.

```
bun run modules:wasm                    # -> dist/wasm-modules/<id>.tar
# then upload the .tar in the admin Store (Settings -> Store -> Upload bundle)
```

The server unpacks it into `<data>/modules/<id>/`, loads it live, serves its page
same-origin, and it survives restarts. A WASM module is sandboxed
request/response logic (capabilities + HTTP) -- it cannot be a live background
service (those stay compiled in).

## Folder layout

A module folder is:

```
<id>/
  module.json      # manifest: id, deps, capabilities, config, feRemote
  server/          # Rust backend (an EmbeddedModule `MODULE` const, + extras)
  ui/              # React frontend (a LumaModule: pages, nav, settings)
  locales/         # en.json, fr.json (this module's translations)
  icon.svg
  README.md
```

## Pages + sections

A module page is a `route` + a `navItem`. Its `section` places its link in a
named nav group: **admin** groups `management | media | acquisition | system |
maintenance` (or `admin` for the generic "Module pages" group), or `library` for
the main sidebar. `icon` is a name (e.g. `download`, `antenna`; see
`clients/web/src/modules/module-icons.ts`), `requires` gates it by capability.

```ts
navItems: [{ to: '/admin/m/torrents', label: 'nav.title', icon: 'download',
             section: 'acquisition', requires: 'library.manage' }]
```

## i18n

Ship `locales/{en,fr}.json` and pass them as the module's `locales`. `label`s and
`host.i18n.t(key)` resolve against the module's OWN catalog first, then the core
catalogs -- no change to the app's typed key union. (Single-file modules: use
` ```locale.en ` / ` ```locale.fr ` blocks.)

## Dependencies

`dependsOn` is a hard dependency (a bare id, `"id@^1.0"`, or `{ id, version }`
with a semver range enforced on the backend). `optionalDependsOn` is ordered
first when present but not required. `requires: [{ kind, id? }]` is a capability
dependency satisfied by any providing module. Status is shown per module in
Admin > Modules.

## Conventions

- **id**: reverse-DNS, `^[a-z0-9]+(\.[a-z0-9-]+)+$` (e.g. `dev.luma.notes`). It is
  the join key across backend/frontend and the schema enforces it.
- **`MODULE`**: every compiled-in module crate exports one `pub const MODULE`.
- **`provides`**: a manifest's `provides` (capabilities) is a *declaration* for
  introspection (`GET /api/modules`) + capability deps; the concrete dispatch is
  a sub-engine registry (e.g. `DownloadClientRegistry`).
- Manifests are validated against `modules/module.schema.json` by
  `bun run modules:validate` (covers `server/modules/*`, `wasm-modules/*`, and
  `modules/*.module.md`).
