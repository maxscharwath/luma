# Installing KROMA on your devices

How to get each client running on real hardware: TVs need **developer mode**
enabled once, macOS needs the **quarantine** cleared once. Nothing here requires
a store account or a paid certificate.

## Where to get the builds

- **GitHub Releases** (a `vX.Y.Z` tag): every artifact attached to one release
  `.spk` (Synology), `.dmg` (macOS), `-setup.exe` / `.msi` (Windows),
  `.AppImage` / `.deb` (Linux / Steam Deck), `.ipk` (LG webOS),
  `.wgt` (Samsung Tizen), `.apk` (Android TV).
- **Prebuilt from GitHub Actions** without tagging a release: see below.
- **Locally**: `bun install && bun run build:tv` (all TV bundles) or the
  per-client commands in each `clients/*/README.md`.

### Prebuilt installers from Actions (no release needed)

The **Build & Release** workflow can be run by hand and produces the exact same
installable packages as a tagged release, just as run artifacts instead of
release assets:

1. GitHub > **Actions** > **Build & Release** > **Run workflow**. Pick the
   scope with `targets`: `all`, `tv` (.ipk + .wgt + .apk), `desktop`
   (.dmg + .exe/.msi + .AppImage/.deb) or `spk`; `version` is optional
   (defaults to `server/Cargo.toml`).
2. When the run is green, open it and scroll to **Artifacts**:
   `kroma-webos-ipk`, `kroma-tizen-wgt`, `kroma-androidtv-apk`,
   `kroma-desktop-macos|windows|linux`, `kroma-synology-spk`, `kroma-web`.
3. Download the one you need. GitHub wraps every artifact in a **.zip**:
   unzip it first, the installable file (`.ipk`, `.wgt`, `.apk`, `.dmg`, ...)
   is inside. Then follow the per-device steps below.

Same thing from the CLI:

```bash
gh workflow run "Build & Release" -f targets=tv   # or all / desktop / spk
gh run watch                                      # wait for it to finish
gh run download -n kroma-webos-ipk                 # unzipped automatically
gh run download -n kroma-androidtv-apk
gh run download -n kroma-tizen-wgt
```

Heads-up: the **CI** workflow (every push/PR) also uploads artifacts, but those
are the raw web bundles (`dist/` folders) for debugging, NOT installable
packages. For something you can put on a device, use **Build & Release**
artifacts or a tagged release. Artifacts expire (90 days by default; CI ones
after 7); releases stay forever.

Also: the **`desktop-latest`** release you may find on the Releases page is the
rolling **desktop auto-update channel** (macOS/Windows/Linux installers + the
`latest.json` the installed apps poll). It never contains TV packages get
those from a Build & Release run or a `vX.Y.Z` release.

Install the **server** first (Synology `.spk`, Docker image
`ghcr.io/<owner>/kroma`, or `cargo` see [server/README.md](server/README.md));
every client asks for the server address on first launch and remembers it.

---

## Samsung TV (Tizen) `.wgt`

One-time **developer mode** on the TV:

1. Open the **Apps** panel on the TV.
2. With the remote, type **1 2 3 4 5** (a hidden shortcut; use the on-screen
   number pad if your remote has no digits). A "Developer mode" popup appears.
3. Switch **Developer mode ON**, enter the **IP of your computer** (the machine
   that will push the app), and restart the TV.

### Install a prebuilt `.wgt` (from Actions / a release)

There is **no npm package** for Samsung's tooling (unlike LG's
`@webos-tools/cli`): the `tizen` and `sdb` commands only ship with
**Tizen Studio**. You do NOT need the IDE though - the **CLI-only**
installer is enough, and installing a prebuilt `.wgt` needs **no
certificate** (ours is already signed; certificates are only required to
*package*):

```bash
# 1. Get the .wgt
gh run download -n kroma-tizen-wgt          # or download it from a release

# 2. Tizen Studio CLI, one-time headless install (~400 MB; needs a JDK)
#    Linux:  web-cli_Tizen_Studio_6.0_ubuntu-64.bin
#    macOS:  web-cli_Tizen_Studio_6.0_macos-64.bin
wget "https://download.tizen.org/sdk/Installer/tizen-studio_6.0/web-cli_Tizen_Studio_6.0_ubuntu-64.bin"
chmod +x web-cli_*.bin && ./web-cli_*.bin --accept-license --no-java-check "$HOME/tizen-studio"
export PATH="$PATH:$HOME/tizen-studio/tools:$HOME/tizen-studio/tools/ide/bin"

# 3. Connect to the TV (developer mode above must list this computer's IP)
sdb connect 192.168.1.50
sdb devices                                # note the device id, e.g. UE50AU7172...

# 4. Install
tizen install -n KROMA.wgt -t <device-id>
```

### Or build + deploy from the repo

```bash
make -C clients/tizen deploy TV_IP=192.168.1.50   # build + sign + install + launch
```

Notes:
- The release `.wgt` is signed with a **throwaway developer certificate**. A TV
  in developer mode accepts it, but if a KROMA build signed with a *different*
  certificate is already installed, uninstall that one first (Apps > long-press
  the KROMA tile > Delete), or the install fails with a signature error.
- Developer mode survives reboots; the app stays installed like any other.
- Community GUI installers that skip Tizen Studio exist (they speak the sdb
  protocol directly), but nothing official or maintained enough to recommend
  here; the CLI-only install above is the reliable minimal path.

## LG TV (webOS) `.ipk`, old and new models

The single `.ipk` covers **every supported generation** (webOS 4.x from 2018 up
to current): it carries both the modern and the legacy bundle and picks the
right one at launch. One-time **Developer Mode** on the TV:

1. Create a (free) account on [developer.lge.com](https://developer.lge.com).
2. On the TV, install the **Developer Mode** app from the LG Content Store and
   log in with that account.
3. In the app, switch **Dev Mode Status ON** (the TV restarts), then switch
   **Key Server ON**.

Then, from a computer on the same network - end to end, starting from a
prebuilt Actions artifact:

```bash
# 1. Get the .ipk (from a Build & Release run, or download it from a release)
gh run download -n kroma-webos-ipk      # -> tv.kroma.webos_<version>_all.ipk

# 2. Get the webOS CLI (once)
bun add -g @webos-tools/cli            # or: npm install -g @webos-tools/cli

# 3. Register the TV: IP + the passphrase shown in the Developer Mode app
ares-setup-device -a tv -i "host=192.168.1.50" -i "port=9922" \
  -i "username=prisoner" -i "passphrase=ABC123"
# (or run `ares-setup-device` with no flags for the interactive wizard)

# 4. Install + launch
ares-install tv.kroma.webos_*_all.ipk -d tv
ares-launch tv.kroma.webos -d tv
```

Notes:
- Dev Mode sessions last **50 hours**; open the Developer Mode app and press
  the extend button (or just relaunch it) to renew. If it expires, sideloaded
  apps disappear until Dev Mode is re-enabled reinstall the `.ipk` after.
- 2016-17 models (webOS 3.x) are not supported; 2018+ (webOS 4.0) and newer are.

## Android TV / Google TV / Nvidia Shield `.apk`

One-time **developer options** on the device:

1. **Settings > System (or Device Preferences) > About**, scroll to
   **Android TV OS build** and click it **7 times** "You are now a developer".
2. Back in Settings, open **Developer options** and enable
   **USB debugging** and/or **Network debugging** (name varies per device).

Then, from a computer with [adb](https://developer.android.com/tools/adb):

```bash
adb connect 192.168.1.60:5555          # accept the prompt shown on the TV
adb install -r KROMA-androidtv-0.1.0.apk
```

The app appears in the normal apps row (it registers as a Leanback TV app).
Alternative without a computer: the **Downloader** app (allow it in
"unknown sources") can fetch the `.apk` from any URL, e.g. the GitHub release.

### Google Chromecast with Google TV (4K / HD)

The Chromecast with Google TV is an Android TV device: the same `.apk`
installs and gets full hardware HEVC decode (the pre-Google-TV cast-only
dongles are NOT supported: no apps, and no HEVC on most of them). The menu
paths on the Google TV UI:

1. **Settings > System > About > Android TV OS build**: click it **7 times**
   with the remote until "You are now a developer!" appears.
2. **Settings > System > Developer options** (now visible) > enable
   **USB debugging** (this also allows debugging over the network there is
   no usable USB data port anyway).
3. Get the dongle's IP: **Settings > Network & Internet** > your Wi-Fi.
4. From a computer on the same network:

```bash
adb connect 192.168.1.61:5555     # a confirmation prompt appears on the TV:
                                  # tick "always allow" and accept
adb install -r KROMA-androidtv-0.1.0.apk
```

Without a computer: install **Downloader** (by AFTVnews) from the Play Store
on the Chromecast, allow it under **Settings > Apps > Security & restrictions
> Unknown sources**, then enter the direct URL of the `.apk` from a GitHub
release (Actions artifacts won't work there: they need a GitHub login and are
zipped). KROMA shows up under "Your apps" like any installed app; storage is
tight on these dongles (8 GB) but the app is only ~4 MB.

Notes:
- Release APKs are debug-signed unless the repo's Android keystore secrets are
  configured. Android refuses to update an app whose signature changed if an
  install fails with a signature error: `adb uninstall tv.kroma.androidtv`, then
  install the new one.

## macOS `.dmg` (and removing the quarantine)

The app is not notarized (no paid Apple developer account), so the **first**
launch trips Gatekeeper: "KROMA is damaged / can't be opened". This is only the
quarantine flag macOS puts on downloaded files. Two ways to clear it:

**Option A settings toggle:**

1. Double-click `KROMA.app` once (it will be blocked, that's expected).
2. Open **System Settings > Privacy & Security**, scroll down to the message
   about KROMA, click **Open Anyway**, and confirm.

**Option B terminal (fastest):** after dragging `KROMA.app` to Applications:

```bash
xattr -dr com.apple.quarantine /Applications/KROMA.app
```

Then it opens normally. This is needed **once per machine**: the built-in
auto-updater installs future versions without quarantine, so updates are
silent from then on.

## Windows `.exe` / `.msi`

The installer is unsigned, so SmartScreen shows "Windows protected your PC":
click **More info > Run anyway**. Once installed, the app self-updates
silently.

## Linux desktop / Steam Deck `.AppImage` / `.deb`

Desktop Linux: install the `.deb`, or `chmod +x KROMA_*.AppImage` and run it.

Steam Deck:

1. Copy `KROMA_*.AppImage` to the Deck and `chmod +x` it (Desktop Mode).
2. **Steam > Add a Non-Steam Game > Browse** and pick the AppImage.
3. Launch from Game Mode and set the controller layout to **Gamepad**.
   D-pad/stick = focus, A = OK, B = back, X = play/pause, L/R = seek.

mpv is bundled (the `kroma-mpv` sidecar drives hardware video decode); nothing to
install. To use your own mpv instead, point the `KROMA_MPV` env var at it.

## Synology NAS `.spk`

1. **Package Center > Settings > General > Trust Level**: allow
   **Any publisher** (the package is self-built, not Synology-signed).
2. **Package Center > Manual Install**, pick the `.spk`, follow the wizard.
3. Open KROMA from the main menu; media folders are configured in the app's
   admin console.

## Web browser

Nothing to install: browse to the server (e.g. `http://nas:4040` or your
tunnel URL). The server ships the web app itself.
