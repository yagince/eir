# eir

A menubar app that watches GitHub PRs and Issues — a from-scratch alternative to Trailer / Neat, built with Tauri v2 and SvelteKit. Runs on macOS, Windows, and Linux.

Named after Eir, the Norse goddess of healing. The icon combines a watching eye with a feather.

## Features

- **Menubar / system-tray only** — no taskbar entry; lives as a tray icon with a 380×520 popup (auto-expands to fill screen height on macOS)
- **GitHub OAuth device flow** — sign in once, the token is stored in a per-user config file and survives restarts
- **Grouped list** — PRs and Issues grouped by repository with sticky headers and per-repo unread counts
- **Filter tabs** — All / Mine / Review Requests / Mentions, backed by different GitHub search queries
- **Auto-refresh** — silent background polling every 30s–5min (configurable)
- **Native notifications** — desktop notifications when a new PR or Issue first appears
- **Unread tracking** — macOS tray badge shows the unread count; Windows/Linux show it in the hover tooltip
- **Global shortcut** — `Ctrl+Shift+E` toggles the popup from anywhere
- **Light/dark aware** — template-mode tray icon on macOS and `prefers-color-scheme` styling everywhere

## Install

Grab the latest build for your OS from the [Releases page](https://github.com/yagince/eir/releases/latest).

### macOS

- Apple Silicon (M1/M2/M3/M4): `eir_<version>_aarch64.dmg`
- Intel: `eir_<version>_x64.dmg`

Open the DMG and drag **eir.app** into `/Applications`. The bundle is ad-hoc signed (no Apple Developer ID yet), so on first launch Gatekeeper refuses with `"eir.app" is damaged and can't be opened`. Remove the quarantine attribute once:

```bash
xattr -rd com.apple.quarantine /Applications/eir.app
```

Then launch it normally. Subsequent launches — including after the in-app updater swaps the binary — don't need this step.

### Windows

Download `eir_<version>_x64-setup.exe` (or `.msi`) and run it. On the SmartScreen "Windows protected your PC" dialog, click **More info → Run anyway** — the build isn't signed with a code-signing certificate yet. Token is stored at `%APPDATA%\eir\token`.

### Linux

Download `eir_<version>_amd64.AppImage` and make it executable:

```bash
chmod +x eir_<version>_amd64.AppImage
./eir_<version>_amd64.AppImage
```

Or install the `.deb` on Debian / Ubuntu derivatives:

```bash
sudo apt install ./eir_<version>_amd64.deb
```

A system tray is required. On GNOME, install the [AppIndicator Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension if you don't already have one.

## Updates

eir ships with an in-app updater. Open the popup → gear icon → **Check for updates**. When a newer version is published, it downloads the signed bundle, verifies the ed25519 signature, and relaunches into the new version.

## Development

```bash
pnpm install
pnpm tauri dev       # launches the app (foreground; quit with Cmd-Q)
```

### Other commands

| Command | Purpose |
|---|---|
| `cd src-tauri && cargo check` | Rust-side build check (no GUI, fast) |
| `cd src-tauri && cargo clippy --all-targets -- -D warnings` | Lint |
| `pnpm dev` | Frontend-only Vite server at :1420 |
| `pnpm check` | TypeScript + svelte-check |
| `pnpm tauri build` | Produce a distributable `.app` / `.dmg` |
| `pnpm tauri icon <png>` | Regenerate app icons from a source PNG |

## Stack

- **Tauri v2** — tray icon, global shortcut, and window control live in `src-tauri/src/`
- **SvelteKit (SPA mode)** with Svelte 5 runes — `adapter-static` + `index.html` fallback
- **pnpm** — package manager

## Layout

```
src/routes/+page.svelte       Single-page frontend (sign-in → list → settings)
src-tauri/src/
  lib.rs                      run(), plugins, window-focus handler
  auth.rs                     Device flow commands + keyring persistence
  github.rs                   fetch_watched + query_for_tab
  tray.rs                     Tray setup, toggle_popup, set_tray_badge
  tauri.conf.json             Window config (380x520, hidden on launch, undecorated)
  icons/                      App & tray icons (tray-icon.png is template-mode)
assets/icon-source/           Source PNGs used for icon generation
```

## License

MIT
