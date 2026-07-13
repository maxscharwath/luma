// A module's packaged icon (icon.svg / icon.png next to its module.json) is
// embedded in the server and served at GET /api/modules/<id>/icon. Pass the API
// origin (`apiBase()`) as `baseUrl` so the icon resolves against the server, not
// the page, when they differ (VITE_LUMA_SERVER, window.__LUMA_API__, the desktop
// shell). Defaults to same-origin. Render `<img src={moduleIconUrl(id, base)} />`.

export function moduleIconUrl(id: string, baseUrl = ''): string {
  return `${baseUrl}/api/modules/${encodeURIComponent(id)}/icon`;
}
