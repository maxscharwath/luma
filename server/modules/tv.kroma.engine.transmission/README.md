# Transmission engine

Transmission RPC as a download sub-engine for the Downloads module.

A backend-only capability module: no page, no routes. Its `ServerModule`
(in this module's own `server/` crate, `kroma-transmission`) registers a
`download-client` factory of kind `transmission` on enable and unregisters it on
disable, so toggling it adds or removes Transmission from the download-client
picker. `dependsOn` the Downloads module (`tv.kroma.torrents`), which owns the
registry.

Layout: `server/` (backend crate) + `module.json` (manifest). See
`modules/README.md` for the guide.
