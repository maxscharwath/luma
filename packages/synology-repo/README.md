# @kroma/synology-repo

Everything that makes KROMA installable from Synology's Package Center:

- **`worker/`** the DYNAMIC package source (a Cloudflare Worker, SynoCommunity
  style): paste the bare worker URL under **Package Center > Settings > Package
  Sources** and DSM's POST gets a live catalog assembled from the GitHub
  Releases API (all releases, stable + nightly channels, arch/DSM filtered,
  edge-cached 5 min). Publishing a release is the whole deploy; nothing is
  rebuilt. Browsers hitting the same URL get a landing page listing EVERY
  version. Deploy: `cd worker && bunx wrangler deploy`
  (CI: `.github/workflows/repo-worker.yml`, on worker changes only).
- **`src/gen-catalog.ts`** the STATIC catalog generator (`catalog.json` +
  landing page + icon) for GitHub Pages, kept as a zero-infra fallback.
- **`src/gen-spk-info.ts`** emits the `<spk>.info.json` sidecar CI attaches
  next to every released `.spk` (version, md5, size, description read from the
  package itself); the worker aggregates these.
- **`src/backfill-info.ts`** one-time backfill of those sidecars onto
  pre-existing releases: `bun packages/synology-repo/src/backfill-info.ts`.

Everything is self-contained (reads version + icon straight out of the `.spk`)
and env-driven. Node built-ins only; run with `bun` (or Node).

## Use

```bash
# from the repo root, after building a .spk into clients/synology/dist/
CATALOG_DOWNLOAD_URL="https://github.com/<you>/<repo>/releases/download/v1.2.3/pkg.spk" \
CATALOG_PAGES_URL="https://<you>.github.io/<repo>" \
CATALOG_OUT_DIR=_site \
bun run --filter @kroma/synology-repo gen
```

Or copy `.env.example` to `.env` and just run `bun run --filter @kroma/synology-repo gen`.

## Preview the landing page (live reload)

```bash
bun run --filter @kroma/synology-repo preview   # http://localhost:4321
```

Serves the landing page rendered from `src/landing.template.html` with sample
values and reloads the browser on every save no `.spk` or build needed. `PORT`
overrides the port; `CATALOG_BETA=true` previews the nightly variant.

## Config (env / `.env`)

| Var | Required | Default | Purpose |
| --- | --- | --- | --- |
| `CATALOG_DOWNLOAD_URL` | yes | | URL DSM downloads the `.spk` from (a Release asset). |
| `CATALOG_PAGES_URL` | yes | | Base URL the output is served at (for the icon). |
| `CATALOG_SPK` | no | newest `*.spk` in `./`, `./dist`, `./clients/synology/dist` | The package to describe. |
| `CATALOG_OUT_DIR` | no | `dist/repo` | Where to write `catalog.json` + icon + `index.html`. |
| `CATALOG_NAME` | no | `catalog.json` | Catalog filename (use `nightly.json` for a beta channel). |
| `CATALOG_BETA` | no | `false` | Mark as beta (nightly channel; needs Package Center's beta toggle). |
| `CATALOG_ICON` | no | icon from inside the `.spk` | Override the store icon PNG. |
| `CATALOG_MAINTAINER` / `_URL`, `CATALOG_DISTRIBUTOR` / `_URL`, `CATALOG_CHANGELOG_URL` | no | KROMA values | Store-card metadata. |

## How it's wired for KROMA

`.github/workflows/synology.yml` runs this for both channels in its `pages` job
(`catalog.json` from the latest stable release, `nightly.json` from the rolling
`nightly` prerelease) and deploys to GitHub Pages. See `clients/synology/README.md`.
