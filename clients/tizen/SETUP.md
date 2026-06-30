# Deploying LUMA to a real Samsung TV

The [`Makefile`](./Makefile) automates **build → sign → install → launch**. The
one-time setup below is the part that can't be scripted (it needs the Tizen
tools, your Samsung account, and your TV). Do it once, then `make deploy` is all
you need.

```
make doctor                       # check tools + config
make deploy TV_IP=192.168.1.50    # build, sign, install, launch on the TV
make logs                         # watch the app's console output
```

## Fast path one command for the toolchain

```bash
bash clients/tizen/scripts/bootstrap-macos.sh
```

Installs Rosetta, downloads + opens the Tizen Studio installer, and verifies the
toolchain. You still do the click-through install and the three Samsung-bound
steps below (Developer Mode, the certificate, your TV's IP) nothing can
automate those.

---

## 1. Enable Developer Mode on the TV (1 min)

1. Open **Apps** (Smart Hub).
2. Press `1 2 3 4 5` on the remote → the **Developer Mode** dialog appears.
3. Turn **Developer mode ON**, and for **Host PC IP** enter **this computer's
   IP** (macOS: System Settings → Network).
4. Reboot the TV.

Find the TV's IP under **Settings → General/Network → Network Status → IP
settings** (or your router). You'll pass it as `TV_IP`.

## 2. Install the Tizen CLI

You need the `tizen` and `sdb` commands. Easiest is **Tizen Studio** (you'll need
it once anyway for the certificate in step 3):

1. Download **Tizen Studio (with IDE)** for macOS from
   <https://developer.tizen.org/development/tizen-studio/download>.
   - Apple Silicon: it runs under Rosetta; Java 17+ is required (you have 21).
2. In **Tizen Studio → Package Manager**, install:
   - **Extension SDK → Samsung Certificate Extension**
   - **Extension SDK → TV Extensions** (TV-x.x)
3. Default install path is `~/tizen-studio`. The Makefile auto-detects
   `~/tizen-studio/tools/ide/bin/tizen` and `~/tizen-studio/tools/sdb`. If you
   put it elsewhere, set `TIZEN_HOME` in `.tizen.env`.

> CLI-only alternative: Samsung also ships a CLI-only package, but the Samsung
> **certificate** in step 3 is created through Certificate Manager (GUI), so
> Tizen Studio is the path of least resistance.

## 3. Create a Samsung certificate (retail TVs require this)

Retail Samsung TVs only run apps signed with a **Samsung** author + distributor
certificate, and the distributor cert is tied to your TV's **DUID**. Create it
once:

1. Connect to the TV first so its DUID can be read:
   ```
   make connect TV_IP=<your-tv-ip>
   make devices            # confirm the TV shows up
   ```
2. Open **Tizen Studio → Tools → Certificate Manager → + (new)**.
3. Choose **Samsung**, type **TV**, and follow the wizard:
   - Sign in with your **Samsung account**.
   - Create/ös pick an **Author** certificate.
   - For the **Distributor** certificate, the wizard reads the **DUID of the
     connected TV** make sure the TV is connected (step 1) and select it.
4. Name the **profile** `LUMA` (or set `PROFILE` in `.tizen.env` to whatever you
   name it). This profile is what `make package` signs with.

> A self-signed Tizen cert (`make cert-selfsigned`) only works on the **emulator**,
> not a retail TV that's why the Samsung wizard is required here.

## 4. Configure + deploy

```bash
cp .tizen.env.example .tizen.env     # then edit TV_IP + PROFILE
make doctor                          # everything green?
make deploy                          # build → sign → install → launch 🎉
```

After it's installed, iterate fast with `make redeploy` (re-uses the connection)
and watch logs with `make logs`.

## 5. Point the app at your media server

On first launch the app shows a connection screen enter
`http://<server-ip>:4040`. It persists in `localStorage`, so subsequent launches
go straight to the library. Make sure the server is running and reachable from
the TV's network (`bun run server` on the host, or the Docker image on your NAS).

---

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `sdb` can't connect | Dev Mode off, wrong Host PC IP, or firewall. Re-do step 1 and reboot the TV. Port is `26101`. |
| Install fails: *signature / certificate* | The profile isn't a **Samsung** cert, or the cert's DUID doesn't match this TV. Recreate it (step 3) with the TV connected. |
| App installs but won't launch | Try `make run` again, or `sdb -s <serial> shell 0 was_execute LumaTV0001.LUMA`. Check `make logs`. |
| `tizen: command not found` | Set `TIZEN_HOME` in `.tizen.env`, or add `~/tizen-studio/tools/ide/bin` and `~/tizen-studio/tools` to `PATH`. |
| Black screen / no data | The app can't reach the server. Re-enter `http://<server-ip>:4040` and confirm the TV and server share a network. |
