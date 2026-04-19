# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**eir** is a macOS menubar-only app that watches GitHub PRs and Issues (a Trailer/Neat alternative). Stack: Tauri v2 (Rust) + SvelteKit (TypeScript, SPA mode) + pnpm. Bundle identifier `dev.yagince.eir`.

The device-flow auth, list UI, grouping, filter tabs, auto-refresh, native notifications, tray badge, global shortcut, and a settings panel are all in place.

## Commands

```bash
# Install
pnpm install

# Run the full Tauri app (RUN THIS YOURSELF — it's foreground and blocks the terminal)
pnpm tauri dev

# Fast Rust-side build check (no GUI) — prefer this for compile verification
cd src-tauri && cargo check
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo fmt

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

The app is **menubar-only** — there is no dock icon, no traditional window. UX is a tray icon plus a popup, with a Ctrl+Shift+E global shortcut as an alternate entry point.

### Rust side (`src-tauri/src/`)

Module split:
- `lib.rs` — `run()`: wires plugins (opener, notification, global-shortcut), installs rustls' `aws_lc_rs` provider, manages `AppState`, registers the toggle shortcut, and hosts the window-focus-loss handler that auto-hides the popup (suppressed when `AppState.pinned == true` during the device-flow code-copy window).
- `auth.rs` — `AppState` (`Option<String>` token + `pinned` flag), device-flow commands (`start_device_flow`, `poll_device_flow`, `sign_out`, `set_window_pinned`), plus a simple `token_store` module that persists the token to `$HOME/.config/eir/token` with mode 0600. See the comment on the module for why we avoid the OS keychain (cdhash-based ACLs re-prompt on every rebuild/release). `AppState::with_stored_token()` is called from `run()` so a stored token is loaded at startup.
- `github.rs` — `fetch_watched(tab)` runs a GitHub search with `octocrab`, mapped to a `WatchedItem` array. `query_for_tab` centralises the four tab queries (`all` / `authored` / `review` / `mentions`).
- `tray.rs` — `setup()` builds the tray, `toggle_popup(app)` is the shared entry point for both tray clicks and the global shortcut (positions the window under the tray using `TrayIcon::rect()`, which is a `tauri::Position`/`tauri::Size` enum pair that must be pattern-matched — `Physical` on macOS in practice). `set_tray_badge(count)` updates the menubar title text for the unread count.

Tauri config (`tauri.conf.json`) keeps the main window undecorated, `alwaysOnTop`, 380x520, hidden on launch. Capabilities (`capabilities/default.json`) grant `core:default`, `opener:default`, `notification:default`.

### Frontend (`src/routes/+page.svelte`)

Single Svelte 5 file, runes-based. Rendering is phase-driven:
- `showingSettings` overrides all other phases.
- `phase: "idle" | "pending" | "loaded"` — idle shows the sign-in CTA; pending shows the device code and opens GitHub; loaded shows the grouped list plus the tab bar and footer.

Key state:
- `items` / `prevIds` / `seen: SvelteSet<number>` — `prevIds` is an internal diff anchor for detecting newcomers to fire notifications; `seen` is a `SvelteSet` (reactive) persisted to `localStorage` under `eir.seen`. First load marks everything as seen to suppress notification noise; subsequent loads only notify for `newIds - prevIds`.
- `activeTab` + `refreshMs` + `notifyEnabled` — all persisted (`eir.tab` / `eir.refreshMs` / `eir.notifyEnabled`). `refreshMs` changes trigger `restartRefreshIfRunning()`.
- `$derived.by` computes repo groups from `items`, sorted by most-recent update; each group carries its own unread count for the pill in the sticky header.
- Auto-refresh is a plain `setInterval` managed by `startRefresh` / `stopRefresh` — started on first successful load, cleared on `onDestroy` or sign-out.

Device-flow UX detail: on sign-in click the frontend calls `set_window_pinned(true)` so the popup doesn't hide when the browser steals focus. As soon as the user_code is on the clipboard (auto or manual) it flips to `set_window_pinned(false)`. When `poll_device_flow` resolves with a token, the Rust side calls `window.show()`+`set_focus()` to bring the popup back into view.

### Icons

Two separate icon assets with different requirements:

- **App bundle icons** (`src-tauri/icons/*.png|.icns|.ico`) — regenerate via `pnpm tauri icon <source.png>`.
- **Tray icon** (`src-tauri/icons/tray-icon.png`) — hand-managed, loaded via `include_bytes!`, uses template mode so it **must be monochrome with transparency**. Source: `assets/icon-source/menubar-icon.png`, copy manually after regeneration.

Icon generation uses a chroma-key workflow (nanobanana on solid `#00FF00` green → ImageMagick `-fuzz 35-38%` key-out) because nanobanana can't reliably produce transparent PNGs. Source PNGs live under `assets/icon-source/`.

## Known gotchas

- **rustls CryptoProvider** — `reqwest`, `octocrab`, and other deps pull in both `aws-lc-rs` and `ring`. `run()` installs `aws_lc_rs::default_provider()` explicitly before anything else; removing that will panic on the first TLS handshake.
- **macOS-specific behaviour** — `ActivationPolicy::Accessory`, `icon_as_template`, and the tray-based window positioning all target macOS. Cross-platform support is doable (keyring crate already covers Windows via `windows-native` and Linux via `sync-secret-service`) but tray/window placement will need platform branches.
- **Phase 3 candidates still open** — GitHub Notifications API integration (more accurate "mentioned" semantics), per-PR review/CI status, user-configurable global shortcut. None are started.
