# Acquisition

The acquisition settings page: how releases are picked, the VPN kill switch, and routing indexer searches through the tunnel.

This is a settings *view* (served by the shared `/api/admin/settings?view=acquisition` endpoint), not its own router, so the module ships only the manifest + the page. Disabling it hides the nav + page; the underlying settings stay readable by the flows that use them.

Layout: `ui/` (frontend), `locales/` (i18n), `module.json` (manifest). See `modules/README.md` for the module authoring guide.
