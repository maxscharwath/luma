# Contributing to LUMA

Thanks for your interest in LUMA! This is a self-hosted media-streaming project
a Rust server plus web and TV clients sharing one core and one design language.
Contributions of all sizes are welcome: bug reports, fixes, docs, new platform
shells, and design polish.

## Project layout

LUMA is a [Bun](https://bun.sh) workspace monorepo with a Rust server alongside it.

```
luma/
├─ server/      Rust media server (axum) scan, SQLite, range streaming
├─ packages/    @luma/core · @luma/ui · @luma/tv  (shared logic, UI, 10-foot experience)
├─ clients/     @luma/web · @luma/tizen · @luma/webos  (thin platform shells)
└─ design/      imported design source (tokens, components, guidelines)
```

See the [root README](README.md) for the full architecture and each package's
own README for details.

## Prerequisites

- **Bun** ≥ 1.3 package manager + runner ([why Bun](README.md#prerequisites))
- **Rust** ≥ 1.81 + **ffmpeg/ffprobe** for the server
- Optional, only to package TV apps: **Tizen Studio** (Samsung) · **webOS TV CLI** (LG)

## Getting started

```bash
git clone https://github.com/maxscharwath/luma.git
cd luma
bun install
bun run dev      # media server (:4040) + web client (:5173) together
```

With no media configured, the server seeds demo titles so the UI is populated
immediately. Point it at real files with `LUMA_MEDIA_DIRS=/path/to/media`.

## Before you open a PR

Everything must build and typecheck cleanly:

```bash
bun run typecheck          # all TS packages
bun run build              # all frontends
cd server && cargo build   # server (use `cargo clippy` if you have it)
```

- Keep clients **thin** UI belongs in `@luma/ui`, logic in `@luma/core`, and the
  shared TV experience in `@luma/tv`. Write platform code once.
- Match the existing style: the design language (deep-charcoal + amber, French
  copy, no emoji) is documented in [`design/readme.md`](design/readme.md).
- Keep the server's dependency graph **lean and Rust 1.81-friendly** (see the
  notes in [`server/Cargo.toml`](server/Cargo.toml)).
- Write clear commit messages and describe the *why* in your PR.

## Reporting bugs

Open an issue with:

- what you expected vs. what happened,
- platform (web / Samsung Tizen / LG webOS) and version,
- server logs (`RUST_LOG=debug`) and, for playback issues, the title's codec
  (`hevc` / `h264` / `av1`) plus audio (`ac3` / `eac3` / `aac`).

## License

By contributing, you agree that your contributions will be licensed under the
project's [MIT License](LICENSE).
