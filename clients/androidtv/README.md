# @kroma/androidtv Android TV / Google TV

> Part of the [KROMA](../../README.md) monorepo the Android TV shell.

A native Kotlin app hosting the shared **`@kroma/tv`** experience in a
transparent WebView over a **media3/ExoPlayer** video plane - the same
"video plane behind the page" model as Tizen AVPlay and the Steam Deck's mpv.
Covers Android TV, Google TV, Chromecast with Google TV, and the Nvidia Shield.

## How playback works

The Kotlin shell injects `__KROMA_ANDROID__` (see `android/.../ExoBridge.kt`)
into the page; the web app detects it and routes playback through its
`ExoEngine` (`packages/tv/.../player/exoEngine.ts`):

- **direct** (default): ExoPlayer opens the ORIGINAL file over plain HTTP Range
  hardware HEVC decode, platform surround audio, native seeks, in-place
  audio-language switching. The server does nothing but send bytes.
- **master** (fallback): the server's stream-copy HLS remux, for the rare file
  ExoPlayer cannot open.

D-pad/OK arrive as normal DOM key events; BACK + media keys are re-injected by
`MainActivity` with the key names `@kroma/core`'s remote mapping understands.

## Develop (in a desktop browser)

```bash
bun install
bun run server              # Rust media server :4040
bun run dev:androidtv       # Vite dev server :5176 arrows + Enter as a remote
```

## Build the APK

Requires Android Studio (or an Android SDK + JDK 17). First build generates the
Gradle wrapper if you don't have `gradle` locally, open `android/` in Android
Studio instead.

```bash
bun run build:androidtv     # web bundle -> dist/ -> android/app/src/main/assets/kroma/
cd clients/androidtv/android
gradle wrapper              # once, if ./gradlew does not exist yet
./gradlew assembleDebug     # -> app/build/outputs/apk/debug/app-debug.apk
adb install app/build/outputs/apk/debug/app-debug.apk
```

Notes:
- `usesCleartextTraffic` is enabled: the KROMA server lives on the LAN over
  plain http.
- `res/drawable/tv_banner.xml` is a placeholder launcher banner; replace with
  real 320x180 art before distribution.
- The WebView tier is the modern bundle (Chrome 99+ floor, see
  `tv.target.ts`); playback never depends on the WebView. If devices with a
  stuck WebView show up, flip on `legacyChrome` exactly like `clients/webos`.
