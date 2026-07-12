# qBittorrent engine

qBittorrent WebUI as a download sub-engine for the Downloads module.

A backend-only capability module: no page, no routes. Its `ServerModule`
(in this module's own `server/` crate, `luma-qbittorrent`) registers a
`download-client` factory of kind `qbittorrent` on enable and unregisters it on
disable, so toggling it adds or removes qBittorrent from the download-client
picker. `dependsOn` the Downloads module (`dev.luma.torrents`), which owns the
registry.

Layout: `server/` (backend crate) + `module.json` (manifest). See
`modules/README.md` for the guide.
