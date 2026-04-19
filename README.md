# eir

A macOS menubar app that watches GitHub PRs and Issues — a from-scratch alternative to Trailer / Neat, built with Tauri v2 and SvelteKit.

Named after Eir, the Norse goddess of healing. The icon combines a watching eye with a feather.

## Features

- **Menubar only** — no dock icon; the app lives as a tray icon with a 380×520 popup
- **GitHub OAuth device flow** — sign in once, the token is stored in a mode-0600 file at `~/.config/eir/token` and survives restarts
- **Grouped list** — PRs and Issues grouped by repository with sticky headers and per-repo unread counts
- **Filter tabs** — All / Mine / Review Requests / Mentions, backed by different GitHub search queries
- **Auto-refresh** — silent background polling every 30s–5min (configurable)
- **Native notifications** — desktop notifications when a new PR or Issue first appears
- **Unread tracking** — tray badge shows total unread count; clicking an item marks it read
- **Global shortcut** — `Ctrl+Shift+E` toggles the popup from anywhere
- **Light/dark aware** — template-mode tray icon and `prefers-color-scheme` styling follow the system

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
