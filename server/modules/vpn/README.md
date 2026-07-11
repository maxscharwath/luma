# VPN

Managed WireGuard bridge (any provider) that torrent traffic routes through, with a live seal test.

The backend behavior lives in the host (`server/src/modules/vpn.rs` = the `ServerModule`; routes in `server/src/api/admin/vpn.rs`; bridge in `luma-engine`), so this module folder ships only the manifest + the admin page. The Downloads module `optionalDependsOn` it: enable VPN first so the engine's SOCKS5 points at a live proxy.

Layout: `ui/` (frontend), `locales/` (i18n), `module.json` (manifest). See `modules/README.md` for the module authoring guide.
