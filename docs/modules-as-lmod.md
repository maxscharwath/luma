# Modules as `.lmod` — out-of-process module architecture

Goal: every module in `server/modules/*` ships as an installable `.lmod` file and
runs **out of the base `luma-server` build** — native, fast, simple to author, and
easy to fetch from a registry.

## Why out-of-process (not WASM, not dlopen)

The modules are native Rust with dependencies a sandbox can't run (librqbit,
candle ML, native TLS, real sockets), so **WASM can't host them**. Native dynamic
loading (`.so`/`.dylib`) **can't work on the musl static build** (Synology can't
`dlopen`). The one model that keeps them native *and* runtime-installable is the
HashiCorp-plugin model: **each module is its own native binary; the core spawns
it, supervises it, and reverse-proxies its HTTP.**

## The pieces (built)

- **`luma-module-runtime`** — what a module binary links. `serve(setup, module)`
  is the whole `main()`: it reads the env the supervisor set, opens the shared
  SQLite directly (WAL = multi-process, so `db()`/auth/session need no IPC),
  builds a `RemoteHost` (the out-of-process `HostCtx`), applies the module's
  migrations, wires the module's own services, runs `on_enable`, and serves the
  module's `admin_routes` + a `/_health` probe on the assigned local port.
  Settings/events/jobs go to the core over a small callback API; everything else
  is local.
- **`luma-module-supervisor`** — the core side. `Supervisor` scans
  `<data>/modules/*`, spawns each enabled module's `module` binary with the
  runtime env (id, free localhost port, core URL, a per-process callback token,
  DB path, data dir), tracks `id -> port`, and stop/spawns them. `proxy_to`
  reverse-proxies a request to a module process. `host_router::<HostCtx>(token)`
  serves `/api/_host/*` (setting / settings / events / job / enabled),
  token-authed, resolved against the core's real state.
- **Core integration** — `main.rs` builds the supervisor and `spawn_enabled`s
  installed modules at boot; `api/mod.rs` mounts the callback API and a
  `/api/module/<id>/*` reverse proxy.
- **`bun run modules:pack`** — builds a module's native binary + stages
  `module.json` + `module` (the binary) + `icon` + `fe/` into a gzip `.lmod`.

## Proven end to end

The real core boots, its supervisor spawns the installed `dev.luma.remote` as a
separate process, and `GET /api/module/dev.luma.remote/_health` is reverse-proxied
to that process → `200 ok`. `remote` builds as a standalone binary purely from its
generic `ServerModule<S: HostCtx>` behind `RemoteHost`.

## Remaining work (staged)

1. **Native install path** — repurpose `/api/admin/store/install` to unpack a
   native `.lmod` under `<data>/modules/<id>/` and `supervisor.spawn(id)` (today
   that endpoint still goes through the WASM host).
2. **The coupled cluster** — `torrents`, `acquisition`, `indexer`, `torznab`,
   `vpn`, and the two engines are wired by **9 cross-module ports**. Out-of-process
   these become HTTP: the provider exposes `/_port/<name>/<method>`, the consumer
   resolves a client proxy. Boundary types need serde derives. Hard cases:
   - `DownloadClientHost::register_engine(fn(&mut Registry))` is a raw **function
     pointer** — the engine-plugin model must change to expose the `DownloadClient`
     trait itself as the RPC surface.
   - `AddTorrentReq`/`DownloadClientCtx` carry borrowed bytes + `Arc<dyn Any>`
     (the librqbit handle) — need owned, serde mirrors.
   - the `ports/naming` engine is a **shared compile-time library** (torrents +
     acquisition) — it stays vendored into each process.
3. **Core → module direct calls** — `api/requests.rs`, `discover.rs`,
   `online_subs.rs` call module functions in-process (active downloads, transcribe,
   interactive search); these become proxied/port calls.
4. **Zero-module base build** — drop every module from `roster.yaml` / the
   generated aggregator / the binary deps once each is converted.
5. **Per-platform binaries** — a `.lmod` needs the module binary for each target
   (musl x86_64/aarch64, macOS, …); CI cross-compiles + packs one per platform (or
   a fat `.lmod`), and the supervisor picks the right one.
6. **Registry** — a published catalog (like `packages/synology-repo`) listing every
   module's `.lmod` + metadata + icon, and an in-app Store that browses + one-click
   installs.

## Trade-offs to weigh (the goal says "optimized, fast")

- Each module binary links its own dep tree; the SDK façade currently re-exports
  `luma-engine`, so a naive per-module binary duplicates a lot of code (large
  artifacts, slow builds). Making this lean needs splitting the SDK's engine
  surface into a thin client — a prerequisite for "optimized".
- Cross-module calls that were direct trait calls become localhost HTTP; hot paths
  (e.g. acquisition scoring releases via the scene parser) must stay in-process
  (shared lib) or they get slower, not faster.
