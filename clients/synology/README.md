# KROMA Synology package (`.spk`)

One self-contained package for **x86_64 DSM 7**. It installs a single process that
serves the **API + streaming _and_ the web UI** on one port, plus a bundled static
**ffmpeg/ffprobe**. No Node, no extra dependencies.

```
kroma-<version>-x86_64.spk
 └ /var/packages/kroma/target/
     ├ bin/kroma-server     # Rust: API + streaming + serves the web SPA
     ├ web/                # the static web SPA (served on the same origin)
     └ ffmpeg/{ffmpeg,ffprobe}
```

## Build the package (on your Mac/Linux)

Prereqs: **Docker** (used to cross-compile a static `x86_64-musl` binary no host
Rust cross-toolchain needed), **bun**, and `curl`.

```bash
clients/synology/build.sh 0.1.0
# → clients/synology/dist/kroma-0.1.0-x86_64.spk
```

The script: builds the web SPA → cross-compiles `kroma-server` to
`x86_64-unknown-linux-musl` in a Docker musl image → downloads a static ffmpeg →
assembles the `.spk`. Re-run faster with `SKIP_WEB=1` / `SKIP_RUST=1`.

## Install via the package source (recommended, auto-updates)

KROMA runs a **dynamic package source** (SynoCommunity-style: a Cloudflare Worker,
`packages/synology-repo/worker`) that answers DSM directly and lists EVERY
release, both channels, live from the GitHub Releases API - nothing is rebuilt
or redeployed when a release ships. The `.spk` itself is hosted on the GitHub
Release (GitHub's CDN).

1. **Package Center → Settings → Package Sources → Add**
   - Name: `KROMA`
   - Location: the bare worker URL (e.g. `https://kroma-packages.<account>.workers.dev/`)
2. **Settings → General → Trust Level → Any publisher** (KROMA is not Synology-signed).
3. Open the **Community** tab, install **KROMA**, and follow the wizard (below).

New releases then show an **Update** button automatically. Opening the same URL
in a browser shows a landing page listing every published version with download
links.

**Nightly channel (optional):** every push to `main` publishes a canary `.spk`
to the rolling `nightly` prerelease. Enable *Settings → General → beta packages*
on the SAME source and DSM rides whichever channel is newest (nightlies use a
4th feature segment, so they outrank their stable base until the next tag).

**Static fallback:** the old GitHub Pages catalogs still work and stay
published: `https://maxscharwath.github.io/kroma/catalog.json` (stable) and
`.../nightly.json` (beta).

## Install on the NAS (manual `.spk`)

1. **Package Center → Manual Install →** pick the `.spk`. (It's unsigned/3rd-party,
   so first enable *Package Center → Settings → Trust Level → Any publisher*.)
2. The install **wizard** asks for your **Movies folder(s)** and **TV Shows
   folder(s)** (absolute NAS paths, e.g. `/volume1/video/Films`; colon- or
   semicolon-separated for several, and leave a field empty if you don't have
   that type) and the **port** (default `4040`). The wizard can't browse folders,
   so type the paths here you'll be able to browse your NAS and reorganize
   libraries (add folders, change each library's type) in the web admin under
   **Bibliothèques** after install.
3. Open `http://<nas-ip>:4040/`.

### One manual permission step (required)
The package runs as its own least-privilege user, which can't read your shares by
default. Grant it read access to the media folder:

**Control Panel → Shared Folder → (your media share) → Edit → Permissions →**
set the system user **`kroma`** (or `sc-kroma`) to **Read-only**, then restart the
package.

> Prefer zero setup over least-privilege? Change `conf/privilege` to
> `{ "defaults": { "run-as": "root" } }` and rebuild it then reads everything
> without granting permissions (less safe; your call for a personal NAS).

## Notes
- **x86_64 only** for now (covers DS2xx+/DS9xx+/DS16xx+ etc.). ARM models need an
  `aarch64-musl` build ask and I'll add the target.
- **Libraries:** the wizard's Movies/TV Shows fields just seed your first two
  libraries. Everything else lives in the **web admin > Bibliothèques**, where a
  folder picker lets you add more folders and set each library's type (Movies or
  TV Shows) without editing paths by hand.
- Data (SQLite DB, image cache, logs) lives in `/var/packages/kroma/var/data` by
  default (or the optional data folder you chose in the wizard) and survives
  upgrades.
- The TV apps (Tizen/webOS) connect to this server over the LAN via mDNS, unchanged.

## Publishing a release (and how updates work)

One workflow owns the whole server deliverable: `.github/workflows/synology.yml`.
A `vX.Y.Z` tag builds the stable `.spk` and attaches it (+ a `<spk>.info.json`
sidecar with version/md5/size, read by the dynamic source) to that GitHub
Release; a push to `main` that touches server/web/shared code builds a canary
`.spk` into the rolling `nightly` prerelease. The same run then assembles the
**Docker image from the .spk payload** (no second compile) and pushes it to
ghcr, and regenerates the static Pages catalogs as a fallback. The dynamic
worker source needs nothing: it reads the releases live.

The musl target dir is cached between runs (`synology-v2-*` cache) and the
cross image is digest-pinned, so a warm push build takes minutes, not a cold
~15-minute compile. The full client fleet (desktop + TV + modules) additionally
ships nightly at 03:00 UTC onto the same `nightly` prerelease via
`.github/workflows/release.yml` (skipped when main has not moved).

To iterate on the store landing page without a build, run
`bun run --filter @kroma/synology-repo preview` (live-reload; `CATALOG_BETA=true`
for the nightly variant).

**Version rule (do not regress this):** DSM installs a `.spk` over an existing one
only when the version is **strictly greater**; otherwise it refuses with the
misleading `4521 "invalid file format"`. DSM's manual-install check compares the
dotted **feature** version and IGNORES the `-build` suffix (proven on a real NAS:
two `0.1.2-<build>` spks read as "same version already installed"). So `build.sh`
stamps `X.Y.Z.BUILD-BUILD` with `BUILD` in a **4th feature segment** (`BUILD` =
minutes since 2020, monotonic) this way every build is strictly newer, including
tag-less **nightlies** that share the same `X.Y.Z`. An earlier commit broke this by
shrinking that segment `0.1.3.<minutes>` (~3.4M) &rarr; `0.1.3.<days>` (~2.4k), so
any NAS with the big-numbered build saw every new build as a **downgrade** and
rejected it forever. Two rules: (1) the base was bumped to `0.1.4` so it outranks
every poisoned `0.1.3.*` at the micro segment; (2) **never shrink the counter's
magnitude** the version number must only ever go up.
