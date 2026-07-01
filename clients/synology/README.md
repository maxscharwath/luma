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

## Install on the NAS

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
