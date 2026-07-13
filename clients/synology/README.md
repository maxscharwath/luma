# LUMA Synology package (`.spk`)

One self-contained package for **x86_64 DSM 7**. It installs a single process that
serves the **API + streaming _and_ the web UI** on one port, plus a bundled static
**ffmpeg/ffprobe**. No Node, no extra dependencies.

```
luma-<version>-x86_64.spk
 └ /var/packages/luma/target/
     ├ bin/luma-server     # Rust: API + streaming + serves the web SPA
     ├ web/                # the static web SPA (served on the same origin)
     └ ffmpeg/{ffmpeg,ffprobe}
```

## Build the package (on your Mac/Linux)

Prereqs: **Docker** (used to cross-compile a static `x86_64-musl` binary no host
Rust cross-toolchain needed), **bun**, and `curl`.

```bash
clients/synology/build.sh 0.1.0
# → clients/synology/dist/luma-0.1.0-x86_64.spk
```

The script: builds the web SPA → cross-compiles `luma-server` to
`x86_64-unknown-linux-musl` in a Docker musl image → downloads a static ffmpeg →
assembles the `.spk`. Re-run faster with `SKIP_WEB=1` / `SKIP_RUST=1`.

## Install via the package source (recommended, auto-updates)

LUMA publishes a **Synology package source** on GitHub Pages, so you install and
**auto-update** straight from Package Center no manual `.spk` uploads. The `.spk`
itself is hosted on the GitHub Release (GitHub's CDN); Pages only serves a small
catalog. Nothing to host, no server.

1. **Package Center → Settings → Package Sources → Add**
   - Name: `LUMA`
   - Location: `https://maxscharwath.github.io/luma/catalog.json`
2. **Settings → General → Trust Level → Any publisher** (LUMA is not Synology-signed).
3. Open the **Community** tab, install **LUMA**, and follow the wizard (below).

New releases then show an **Update** button automatically.

**Nightly channel (optional):** every push to `main` publishes a nightly `.spk` to
a separate beta catalog. To ride nightlies, add
`https://maxscharwath.github.io/luma/nightly.json` as a second source and enable
*Settings &rarr; General &rarr; beta packages*. Note the two catalogs describe the
same `luma` package, and nightlies share the stable base version (`0.1.4.<build>`),
so a nightly is always version-higher a NAS with *both* sources added rides the
nightly. Subscribe to one channel, not both.

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
set the system user **`luma`** (or `sc-luma`) to **Read-only**, then restart the
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
- Data (SQLite DB, image cache, logs) lives in `/var/packages/luma/var/data` by
  default (or the optional data folder you chose in the wizard) and survives
  upgrades.
- The TV apps (Tizen/webOS) connect to this server over the LAN via mDNS, unchanged.

## Publishing a release (and how updates work)

One workflow owns the whole Synology deliverable: `.github/workflows/synology.yml`.
A `vX.Y.Z` tag builds the stable `.spk` and attaches it to that GitHub Release; a
push to `main` builds a nightly `.spk` into the rolling `nightly` prerelease. Either
way the same run then regenerates BOTH catalogs `catalog.json` (stable) +
`nightly.json` (beta) from the latest releases via the `@luma/synology-repo`
package and deploys them to GitHub Pages. NASes on either source update
automatically. (Stable + nightly share one build job, so there is no separate
Pages workflow to keep in sync.)

To iterate on the store landing page without a build, run
`bun run --filter @luma/synology-repo preview` (live-reload; `CATALOG_BETA=true`
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
