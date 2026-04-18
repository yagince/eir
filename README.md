# eir

A macOS menubar app that watches GitHub PRs and Issues — a from-scratch alternative to Trailer / Neat, built with Tauri v2 and SvelteKit.

Named after Eir, the Norse goddess of healing. The icon combines a watching eye with a feather.

## Features

- **Menubar only** — no dock icon; the app lives as a tray icon with a popup
- **Click to toggle** — left-click the tray to open a popup directly below it; it auto-hides on blur
- **Light/dark aware** — template-mode tray icon follows the system; the frontend uses `prefers-color-scheme`

Scaffolding is in place. GitHub integration is not implemented yet.

## Development

```bash
pnpm install
pnpm tauri dev       # launches the app (foreground; quit with Cmd-Q)
```

### Other commands

| Command | Purpose |
|---|---|
| `cd src-tauri && cargo check` | Rust-side build check (no GUI, fast) |
| `pnpm dev` | Frontend-only Vite server at :1420 |
| `pnpm check` | TypeScript + svelte-check |
| `pnpm tauri build` | Produce a distributable `.app` / `.dmg` |
| `pnpm tauri icon <png>` | Regenerate app icons from a source PNG |

## Stack

- **Tauri v2** — tray icon and window control live in `src-tauri/src/lib.rs`
- **SvelteKit (SPA mode)** — `adapter-static` with `index.html` fallback
- **pnpm** — package manager

## Layout

```
src/                  SvelteKit frontend
src-tauri/            Tauri / Rust backend
  src/lib.rs          Tray + window logic
  tauri.conf.json     Window config (380x520, hidden on launch, undecorated)
  icons/              App & tray icons
assets/icon-source/   Source PNGs used for icon generation
```

## License

MIT
