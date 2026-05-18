# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**eir** is a menubar / system-tray app that watches GitHub PRs and Issues (a Trailer/Neat alternative). Stack: Tauri v2 (Rust) + SvelteKit (TypeScript, SPA mode) + pnpm. Bundle identifier `dev.yagince.eir`.

Targets macOS (arm64 + x64), Windows x64, and Linux x64. CI and the release workflow build all four targets. macOS is the primary development and testing target — non-macOS code paths exist but receive less real-world coverage.

Shipped features: OAuth device-flow auth, grouped list UI, filter tabs (all / mine / review / mentions / hidden), repo grouping with per-repo overrides, auto-refresh, native notifications, tray badge, configurable global shortcut, GitHub Notifications API integration, in-app updater, autostart, settings panel.

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
cd src-tauri && cargo test --lib

# Frontend-only dev server at :1420 (no Tauri shell)
pnpm dev

# Frontend type + svelte-check
pnpm check

# Frontend tests (vitest)
pnpm test

# Production bundle (.dmg on macOS, .msi/-setup.exe on Windows, .AppImage/.deb on Linux)
pnpm tauri build

# Regenerate app icons from a source PNG (does not touch tray icon)
pnpm tauri icon assets/icon-source/eir-app-final.png
```

Do not run `pnpm tauri dev` in the background — exit code 0 means the user quit the app, not that it finished successfully, and backgrounding hides the GUI behavior you'd be verifying. For behavioral verification, ask the user to run it and report back.

## Architecture

The app is **menubar / system-tray only** — no dock or taskbar icon, no traditional window. UX is a tray icon plus a popup, with a Ctrl+Shift+E global shortcut (default; user-rebindable) as an alternate entry point.

### Rust side (`src-tauri/src/`)

Module split:
- `lib.rs` — `run()`: wires plugins (opener, dialog, notification, updater, autostart, global-shortcut), installs rustls' `aws_lc_rs` provider, manages `AppState` + `BackgroundHandle`, registers the toggle shortcut, spawns the background fetch worker, and hosts the window-focus-loss handler that auto-hides the popup (suppressed when `AppState.pinned == true` during the device-flow code-copy window or settings dialogs). `ActivationPolicy::Accessory` is set on macOS only — other platforms rely on `skipTaskbar: true` for equivalent behavior.
- `auth.rs` — `AppState` (`Option<String>` token + `pinned` flag), device-flow commands (`start_device_flow`, `poll_device_flow`, `sign_out`, `set_window_pinned`, `set_dialog_mode`), plus a `token_store` module that persists the token to `$HOME/.config/eir/token` (unix, mode 0600) or `%APPDATA%\eir\token` (Windows). See the comment on the module for why we avoid the OS keychain (cdhash-based ACLs re-prompt on every rebuild/release). `AppState::with_stored_token()` is called from `run()` so a stored token is loaded at startup.
- `github.rs` — `fetch_watched(tab)` runs a GitHub search with `octocrab`, mapped to a `WatchedItem` array. `query_for_tab` centralises the four tab queries (`all` / `authored` / `review` / `mentions`). Also exposes `fetch_notifications`, `fetch_item_states`, and `mark_notification_read` for the Notifications-API enrichment path.
- `background.rs` — Long-lived Rust worker that drives polling, diff detection, tray badge updates, and notification emission. Holds `BackgroundHandle` (config + cached state) and emits `state-updated` events to the frontend so the popup can re-render without owning its own timer (WKWebView throttles JS timers when hidden).
- `diff.rs` — Pure functions for computing what changed between two `WatchedItem` snapshots — used by `background.rs` to decide which notifications to fire and how to label them ("New comment", "CI failed", "Updated", etc.).
- `tray.rs` — `setup()` builds the tray, `toggle_popup(app)` is the shared entry point for both tray clicks and the global shortcut (positions the window under the tray using `TrayIcon::rect()`, which is a `tauri::Position`/`tauri::Size` enum pair that must be pattern-matched). `set_tray_badge(count, has_unread)` shows the unread count as an adjacent label + red-dot badge variant on macOS, and as a hover tooltip on Windows/Linux. Tray icon is monochrome template-mode on macOS, colored `32x32.png` elsewhere.
- `shortcut.rs` — User-configurable global shortcut. Stored alongside other settings; loaded at startup and re-registered when changed.
- `settings_io.rs` — `read_text_file` / `write_text_file` commands so the frontend can persist settings JSON without depending on `plugin-fs`.
- `updater.rs` — `relaunch_app` command. On macOS, shells out to `/usr/bin/open -n <bundle>` so the new instance goes through Launch Services (the default `AppHandle::restart` spawns `current_exe` directly, which silently fails for `ActivationPolicy::Accessory` menubar apps after the updater swaps the .app bundle). On other platforms, falls back to `app.restart()`.

Tauri config (`tauri.conf.json`) keeps the main window undecorated, `alwaysOnTop`, 440x680, hidden on launch, `skipTaskbar: true`. Capabilities (`capabilities/default.json`) grant `core:default`, `opener:default`, `dialog:default`, `notification:default`, `updater:default`, `autostart:default`.

### Frontend (`src/routes/+page.svelte`)

Single Svelte 5 file, runes-based. Rendering is phase-driven:
- `showingSettings` overrides all other phases.
- `phase: "bootstrapping" | "idle" | "pending" | "loaded"` — bootstrapping is the initial "restoring state from disk" state, idle shows the sign-in CTA, pending shows the device code and opens GitHub, loaded shows the grouped list plus the tab bar and footer.

Auto-refresh and diff/notification logic live on the Rust side (`background.rs`). The frontend listens for `state-updated` events and re-renders from the payload — it does not own a JS timer (WKWebView throttles JS timers while the popup is hidden, so polling from Svelte would stall).

Key state:
- `items: WatchedItem[]` — replaced wholesale whenever a `state-updated` event arrives. The Rust worker keeps its own `prev_thread_updated_at` map to detect what changed; the frontend just renders.
- `hiddenItems` / `pinnedItems` / `watchedOrgs` — `SvelteSet`s persisted to `localStorage` via helpers in `$lib/storage`.
- `repoSettings: SvelteMap<string, RepoSetting>` — per-repo PR/Issue inclusion overrides. Changes are pushed to the Rust worker via `set_background_config` so the query set is updated immediately.
- `activeTab` + `refreshMs` + `notifyEnabled` + global PR/Issue includes — all persisted; changes trigger a Rust-side config push.
- `$derived.by` computes repo groups from `items`, sorted by most-recent update; each group carries its own unread count for the pill in the sticky header.

Device-flow UX detail: on sign-in click the frontend calls `set_window_pinned(true)` so the popup doesn't hide when the browser steals focus. As soon as the user_code is on the clipboard (auto or manual) it flips to `set_window_pinned(false)`. When `poll_device_flow` resolves with a token, the Rust side calls `window.show()`+`set_focus()` to bring the popup back into view. The same `set_dialog_mode` / `set_window_pinned` mechanism is used to keep the popup visible while native dialogs (Settings save/load, update confirmation) are open.

### Icons

Two separate icon assets with different requirements:

- **App bundle icons** (`src-tauri/icons/*.png|.icns|.ico`) — regenerate via `pnpm tauri icon <source.png>`.
- **Tray icon** (`src-tauri/icons/tray-icon.png`) — hand-managed, loaded via `include_bytes!`, uses template mode so it **must be monochrome with transparency**. Source: `assets/icon-source/menubar-icon.png`, copy manually after regeneration.

Icon generation uses a chroma-key workflow (nanobanana on solid `#00FF00` green → ImageMagick `-fuzz 35-38%` key-out) because nanobanana can't reliably produce transparent PNGs. Source PNGs live under `assets/icon-source/`.

## Known gotchas

- **rustls CryptoProvider** — `reqwest`, `octocrab`, and other deps pull in both `aws-lc-rs` and `ring`. `run()` installs `aws_lc_rs::default_provider()` explicitly before anything else; removing that will panic on the first TLS handshake.
- **macOS updater relaunch** — `AppHandle::restart()` / `tauri-plugin-process`'s `relaunch()` spawn `current_exe()` directly. For an `Accessory` menubar app this bypasses Launch Services and silently fails right after the updater swaps the .app bundle (the old process exits, no new menubar icon). Use `updater::relaunch_app` instead, which goes through `/usr/bin/open -n` on macOS.
- **Cross-platform parity** — Code branches exist for Windows and Linux (token path, tray icon variant, tray label vs. tooltip, popup sizing, updater relaunch), and CI builds all four targets. macOS is the primary test surface — non-mac paths are likely to have rough edges (e.g., Linux tray availability depends on the DE; GNOME needs the AppIndicator extension). Treat non-mac bugs as in-scope but possibly untested rather than out-of-scope.
- **No code signing on Windows/Linux** — `signingIdentity: "-"` is macOS ad-hoc. Windows users hit SmartScreen on first launch; Linux AppImage/.deb is unsigned. Real signing certs aren't set up yet.
