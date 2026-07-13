# @luma/synology-repo

Generates a **Synology package-source repository** (`catalog.json` + a landing
page + the store icon) from a built `.spk`, for static hosting. Add the resulting
catalog URL under **Package Center > Settings > Package Sources** and the package
installs + auto-updates from the **Community** tab no server required.

It is self-contained (reads the version and icon straight out of the `.spk`) and
fully env-driven, so it works for any package, not just LUMA. Node built-ins only;
run it with `bun` (or Node).

## Use

```bash
# from the repo root, after building a .spk into clients/synology/dist/
CATALOG_DOWNLOAD_URL="https://github.com/<you>/<repo>/releases/download/v1.2.3/pkg.spk" \
CATALOG_PAGES_URL="https://<you>.github.io/<repo>" \
CATALOG_OUT_DIR=_site \
bun run --filter @luma/synology-repo gen
```

Or copy `.env.example` to `.env` and just run `bun run --filter @luma/synology-repo gen`.

## Preview the landing page (live reload)

```bash
bun run --filter @luma/synology-repo preview   # http://localhost:4321
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
| `CATALOG_MAINTAINER` / `_URL`, `CATALOG_DISTRIBUTOR` / `_URL`, `CATALOG_CHANGELOG_URL` | no | LUMA values | Store-card metadata. |

## How it's wired for LUMA

`.github/workflows/synology.yml` runs this for both channels in its `pages` job
(`catalog.json` from the latest stable release, `nightly.json` from the rolling
`nightly` prerelease) and deploys to GitHub Pages. See `clients/synology/README.md`.
