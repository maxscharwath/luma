# Remote access

Public HTTPS access: the share / Quick Connect URL plus an optional managed Cloudflare Tunnel (`cloudflared`) connector.

The backend lives entirely in this module's own `server/` crate (`kroma-remote`): the `ServerModule`, its admin routes, and the managed Cloudflare Tunnel (`cloudflared`) connector. It reaches the app only through the `HostCtx` seam.

Layout: `server/` (backend crate), `ui/` (frontend), `locales/` (i18n), `module.json` (manifest). See `modules/README.md` for the module authoring guide.
