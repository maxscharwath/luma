# Module / core decoupling: target architecture

Status: IMPLEMENTED (branch `luma-modules-poc`). This was the migration plan; it
has since been carried out in full. The core (`luma-engine`) and every
`server/crates/*` foundation crate now depend on zero module crates; the roster
is generated from `modules/roster.yaml`; `server/src/modules/` is deleted. The
sections below are kept as the design record (the "before" they describe is the
pre-migration state).

## 1. Principle

Onion / hexagonal. The **core** knows nothing about any module. A module is a
self-contained vertical (its routes, lifecycle, DTOs, DB schema, events, services)
that plugs into the core through **generic ports** only. Cross-module talk goes
port-to-port, never through the core.

Concretely, three rules the build must enforce:

1. **No core crate may depend on a module crate.** (`luma-engine`, `luma-domain`,
   `luma-db`, `luma-primitives`, `luma-config`, `luma-i18n`, `luma-http`, the
   ports crates: none may name `luma-downloads`, `luma-vpn`, `luma-indexer`, ...)
2. **No core crate may contain module-specific types** (no `DownloadProgress`,
   `ManualSearchView`, `download_clients` table, `vpn_*` helpers in core).
3. **`server/src` (the binary shell) names no module** beyond the generated
   composition roster.

## 2. Where we are now (the violations)

Module-specific code currently living in core:

| Core crate | Module pollution to remove |
|---|---|
| `luma-module-host` (port) | `HostEvent::{DownloadProgress, DownloadCompleted, VpnStatus, RequestUpdated}`, `vpn_wg_configured`, `vpn_proxy_url` |
| `luma-engine` (core) | `AppState.{downloads, vpn, remote}` fields; the `HostEvent -> ServerEvent` mapping; the `services::acquisition` orchestration; **Cargo deps on `luma-indexer`, `luma-torrent`, `luma-remote`, `luma-vpn`, `luma-downloads`** |
| `luma-domain` (core) | acquisition DTOs (`DownloadRow`*, `ManualSearchView`, `VpnAdminView`, `ScoredReleaseView`, ...) |
| `luma-db` (core) | module tables + rows (`download_clients`, `downloads`, `indexers`) |
| `server/src` (shell) | the download admin routes (`api/admin/downloads.rs`, `download_clients.rs`), `api/requests.rs` -> `state.downloads.activate`, the hardcoded roster in `modules/mod.rs` |

Plus a layout wart: `luma-downloads` lives in `server/crates/` though it is the
Downloads module backend; it belongs under `server/modules/`.

## 3. Target layers

Dependencies point inward only (outer may depend on inner, never the reverse):

```
  ┌─────────────────────────────────────────────────────────────┐
  │ COMPOSITION ROOT   server/ (main.rs + api router)            │  names modules
  │   builds AppState + module services, registers them, mounts  │  only via the
  │   the generated roster. No module named in hand-written code. │  generated roster
  ├─────────────────────────────────────────────────────────────┤
  │ ADAPTERS (modules)  server/modules/dev.luma.*                 │  depend on: core-
  │   downloads, indexer, vpn, remote, acquisition, engines, ...  │  foundations +
  │   each owns: routes, lifecycle, DTOs, DB schema, events,      │  ports + (peer
  │   services, and its impls of shared port traits.             │  modules) contracts
  ├─────────────────────────────────────────────────────────────┤
  │ CORE APP   luma-engine                                        │  depends on:
  │   player, library, users, settings, jobs, event bus, service │  foundations +
  │   registry, module-host wiring. Implements HostCtx. No module.│  ports ONLY
  ├─────────────────────────────────────────────────────────────┤
  │ PORTS (contracts)   luma-module-sdk, luma-module-host,        │  generic only
  │   luma-contracts (NEW: peer-to-peer port traits)             │
  ├─────────────────────────────────────────────────────────────┤
  │ FOUNDATIONS   luma-primitives, luma-config, luma-i18n,        │  zero module
  │   luma-http, luma-db (pool+runner+CORE schema), luma-domain   │  knowledge
  │   (CORE domain types only)                                    │
  └─────────────────────────────────────────────────────────────┘
```

### What each layer holds after the migration

- **luma-db**: the `Pool`, the migration runner, and only CORE schema (users,
  sessions, libraries, media items, playback). Modules register their own
  migrations through a port hook; module row-types + queries move to the modules.
- **luma-domain**: only core domain (`User`, `Permission`, library/media/playback
  types). Every acquisition/vpn DTO moves to its module.
- **luma-module-host (the seam / `HostCtx`)**: only generic host capabilities:
  `db()`, `data_dir()`, settings accessors, `require`/`require_any_admin`/`lerr`,
  `module_enabled`, `get_service` (DI), a **generic** `publish(Event)`,
  `trigger_job(key)`, and `register_migrations(sql)`. No `HostEvent` module
  variants, no `vpn_*`. Plus the generic `ServerModule` trait.
- **luma-contracts (NEW)**: thin crate of PORT TRAITS for module-to-module calls
  (e.g. `SearchPort`, `DownloadPort`) and the shared IDs/DTOs those ports pass.
  Both provider and consumer modules depend on it; neither depends on the other.
- **luma-engine**: core app + `AppState` (core services + the generic service
  registry; NO typed module fields). Implements `HostCtx`. Depends on foundations
  + ports only.
- **modules**: each owns its full vertical and depends on foundations + ports +
  (for peer calls) `luma-contracts`. `luma-downloads` moves under
  `server/modules/dev.luma.torrents/server` (or its own module dir).

## 4. Generic mechanisms

### 4.1 Event bus (generic)

Today `HostEvent` (seam) and `ServerEvent` (engine) enumerate module events; the
bus pre-serializes to `{ "type": "...", ...fields }`.

Target: the bus carries an opaque, already-shaped event:

```rust
// luma-module-host
pub struct Event { pub topic: String, pub payload: serde_json::Value }
// HostCtx:
fn publish(&self, event: Event);   // replaces the typed HostEvent publish
```

- A module builds its own event and publishes it:
  `host.publish(Event { topic: "download.progress".into(), payload: json!({...}) })`.
- The core bus fans out `{ "type": <topic>, ...payload }` to WebSocket clients:
  identical wire shape to today, so the frontend is unchanged.
- Core defines NO module event type. `ServerEvent`'s module variants are deleted;
  core events (if any remain, e.g. scan status) either stay as core topics or use
  the same generic path.

Open question E1: keep a small typed enum for genuinely-core events, or make
everything `{topic, payload}`? (Recommendation: everything generic; one path.)

### 4.2 Module-to-module ports (service registry + contracts)

The DI registry (`get_service(TypeId)` + `service::<T>()`) already exists. Use it
for peer calls through trait objects whose trait lives in `luma-contracts`:

```rust
// luma-contracts
pub trait SearchPort: Send + Sync {
    fn manual_search(&self, query: &str) -> anyhow::Result<Vec<ReleaseHit>>;
}
```

- The **indexer** module implements `SearchPort` and registers
  `Arc<dyn SearchPort>` in the DI registry at startup.
- The **downloads/acquisition** module resolves `service::<dyn SearchPort>()` and
  calls it. It never names `luma-indexer`; it names the port.
- Same pattern for "activate a grabbed row" and "re-import": a `DownloadPort` the
  downloads module provides and the requests/acquisition flow consumes.

Host -> module wiring: the composition root constructs each module service and
registers it in the DI registry (so `service::<T>()` and `service::<dyn Port>()`
both resolve).

Open question E2: one shared `luma-contracts` crate for all port traits (simplest,
recommended), vs each module publishing its own tiny `*-contract` crate.

### 4.3 Module-owned DB schema

- `luma-db` keeps the pool + a migration runner that also runs **module
  migrations** collected via the seam: `ServerModule::migrations() -> Option<&str>`
  (or a `register_migrations` hook), run at startup after core migrations.
- Each module owns its tables' SQL + row structs + query fns (in the module crate,
  using `host.db()` for the pool).
- Cross-boundary foreign keys (e.g. `downloads.request_id -> requests`) need a
  decision (open question E3): make `requests` a module too, or keep the link soft
  (no FK), or keep `requests` core and expose a port.

### 4.4 Config-driven roster (no hardcoded module list)

- `modules.toml` (repo, build time) lists every module (id, crate, backend kind:
  manifest-only / server-module / lifecycle-only). Compile-time Rust crates link
  at build time, so this is a BUILD-time manifest, not a runtime data file (only
  WASM/`.tar` modules load from the data dir at runtime, which already works).
- `scripts/gen-modules.ts` reads `modules.toml` and generates, into
  `luma-modules-generated`: the manifest `register_all`, the `server_modules<S>()`
  roster, and the crate's Cargo deps.
- The generic kernel drivers (`mount_admin`, `apply_enabled_states`, resolve/order)
  move into a crate (`luma-module-host` or a new `luma-module-kernel`), driven off
  the generated roster + the seam. `server/src/modules/` is deleted.

## 5. What moves where (concrete)

| Thing | From | To |
|---|---|---|
| `HostEvent` variants | luma-module-host | deleted; modules emit `Event{topic,payload}` |
| `vpn_wg_configured`/`vpn_proxy_url` | luma-module-host | luma-vpn (or luma-contracts if a peer needs it) |
| `ServerEvent` module variants + mapping | luma-engine | deleted (generic bus) |
| acquisition DTOs (`ManualSearchView`, `ScoredReleaseView`, ...) | luma-domain | the acquisition / downloads module |
| `DownloadRow`, `DownloadClientRow` + `download*`/`indexers` tables | luma-db | luma-downloads / luma-indexer |
| `AppState.{downloads,vpn,remote}` fields | luma-engine | removed; via DI registry |
| `services::acquisition` (search/import/grab) | luma-engine | NEW `dev.luma.acquisition` module |
| download admin routes | server/src/api/admin | luma-downloads (module) |
| `api/requests.rs` -> `state.downloads.activate` | server/src | via `DownloadPort` |
| hardcoded roster | server/src/modules/mod.rs | generated (modules.toml) |
| `luma-downloads` crate | server/crates | server/modules/dev.luma.torrents |

## 6. Sequenced migration (each phase compiles + is a commit)

- **A. Generic event bus.** Replace `HostEvent`/`ServerEvent` module variants with
  `Event{topic,payload}`; keep the wire shape; move each event's construction into
  its module. Verify WS + frontend unchanged. (Unblocks removing the seam's module
  events.)
- **B. Contracts crate + peer ports.** Create `luma-contracts`; define `SearchPort`
  + `DownloadPort`; indexer/downloads implement + register; consumers resolve via
  DI. Removes concrete cross-module calls.
- **C. Module-owned DTOs.** Move acquisition/vpn/download DTOs from `luma-domain`
  into their modules (+ update ts-rs bindings source). Core keeps core domain.
- **D. Module-owned DB schema.** Add the migration hook to the seam; move module
  tables/rows/queries out of `luma-db`. Resolve the requests FK (E3).
- **E. Dependency inversion.** Drop `AppState`'s typed module fields; make
  `luma-engine` stop depending on module crates; move `services::acquisition` into
  a new `dev.luma.acquisition` module; relocate the download routes (now unblocked
  by ports). `luma-engine` depends on foundations + ports only.
- **F. Config-driven roster.** `modules.toml` + codegen; kernel drivers to a crate;
  delete `server/src/modules/`; purge `server/src` of module refs; move
  `luma-downloads` under `server/modules/`.
- **G. Enforcement.** A test / CI check that fails if any core crate's Cargo.toml
  names a module crate, or `server/src` names a module type. Makes the boundary
  permanent.

Each phase is independently shippable and leaves the app working.

## 7. Open decisions for you

- **E1 Events:** all events generic `{topic,payload}`, or keep a tiny typed enum
  for core-only events? (Recommend: all generic.)
- **E2 Contracts:** one shared `luma-contracts` crate, or per-module contract
  crates? (Recommend: one shared.)
- **E3 requests<->downloads:** make `requests` a module, keep a soft link (no FK),
  or keep `requests` core behind a `DownloadPort`? (Recommend: soft link now,
  `requests` core; revisit.)
- **E4 Scope/order:** run phases A..G in order over multiple sessions, or
  prioritize a subset first?
- **E5 luma-downloads location:** move under `server/modules/dev.luma.torrents`
  now (rename/move) or after the dependency inversion (Phase E)?

## 8. Risks

- The event-bus change touches the WS fan-out + the frontend event handler; the
  `{type, ...fields}` wire shape must stay byte-compatible.
- ts-rs bindings are generated from the DTO structs; moving DTOs moves the bindings
  source and the `bun run gen:types` output paths.
- Per-module migrations + a shared pool means migration ORDER matters (core first,
  then modules) and cross-table FKs must not cross a module boundary hard.
- This is multiple sessions of careful, verified work; the phase boundaries are the
  checkpoints.
