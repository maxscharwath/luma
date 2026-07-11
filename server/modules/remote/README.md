# Remote access

Public HTTPS access: the share / Quick Connect URL plus an optional managed Cloudflare Tunnel (`cloudflared`) connector.

The backend behavior lives in the host (`server/src/modules/remote.rs` = the `ServerModule`; routes in `server/src/api/admin/remote.rs`; connector in `luma-engine`), so this module folder ships only the manifest + the admin page.

Layout: `ui/` (frontend), `locales/` (i18n), `module.json` (manifest). See `modules/README.md` for the module authoring guide.
