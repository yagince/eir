# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**eir** is a macOS menubar-only app that watches GitHub PRs and Issues (a Trailer/Neat alternative). Stack: Tauri v2 (Rust) + SvelteKit (TypeScript, SPA mode) + pnpm. Bundle identifier `dev.yagince.eir`.

Scaffolding is in place; GitHub integration has not been implemented yet.

## Commands

```bash
# Install
pnpm install

# Run the full Tauri app (RUN THIS YOURSELF — it's foreground and blocks the terminal)
pnpm tauri dev

# Fast Rust-side build check (no GUI) — prefer this for compile verification
cd src-tauri && cargo check

# Frontend-only dev server at :1420 (no Tauri shell)
pnpm dev

# Frontend type + svelte-check
pnpm check

# Production bundle (.app / .dmg)
pnpm tauri build

# Regenerate app icons from a source PNG (does not touch tray icon)
pnpm tauri icon assets/icon-source/eir-app-final.png
```

Do not run `pnpm tauri dev` in the background — exit code 0 means the user quit the app, not that it finished successfully, and backgrounding hides the GUI behavior you'd be verifying. For behavioral verification, ask the user to run it and report back.

## Architecture

The app is **menubar-only** — there is no dock icon, no traditional window. The entire UX is a tray icon plus a popup.

### Rust side — `src-tauri/src/lib.rs`

Single file wires up the whole menubar behavior. Key pieces:

- `ActivationPolicy::Accessory` (macOS) removes the dock icon.
- The window (`label = "main"`) is configured in `tauri.conf.json` as `visible: false`, `decorations: false`, `skipTaskbar: true`, `alwaysOnTop: true`, size 380x520.
- `TrayIconBuilder` sets up the tray with a "Quit eir" menu on right-click.
- Left-click on the tray toggles the popup: positions it centered horizontally below the click point, shows/focuses it. A `WindowEvent::Focused(false)` handler auto-hides on blur — this is what makes it feel like a native menubar popover.
- The tray icon is embedded at compile time via `include_bytes!("../icons/tray-icon.png")` with `icon_as_template(true)` (macOS template mode — the OS colors it for light/dark menubar).

Window positioning math: `x = click.x - window.width/2`, `y = click.y + 8`.

### Frontend — `src/routes/+page.svelte`

SvelteKit in SPA mode (`adapter-static` with `fallback: index.html`). Built output goes to `build/`, which Tauri serves as `frontendDist`. Currently a single placeholder route — add new UI under `src/routes/`.

Dev flow: `pnpm tauri dev` starts `pnpm dev` (Vite at :1420) via `beforeDevCommand`, then Tauri loads that URL.

### Icons

Two separate icon assets with different requirements:

- **App bundle icons** (`src-tauri/icons/*.png|.icns|.ico`) — regenerate via `pnpm tauri icon <source.png>`.
- **Tray icon** (`src-tauri/icons/tray-icon.png`) — hand-managed, loaded via `include_bytes!`, uses template mode so it **must be monochrome with transparency**. Source: `assets/icon-source/menubar-icon.png`, copy manually after regeneration.

Icon generation uses a chroma-key workflow (nanobanana on solid `#00FF00` green → ImageMagick `-fuzz 35-38%` key-out) because nanobanana can't reliably produce transparent PNGs. Source PNGs live under `assets/icon-source/`.

## Next major work

Planned order: GitHub OAuth device flow → PR/Issue fetch (octocrab or GraphQL) → list UI → polling + notifications.
