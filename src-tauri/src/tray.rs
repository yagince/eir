use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Emitter, Manager, PhysicalPosition,
};

/// Event emitted to the webview when the popup transitions from visible to
/// hidden. The frontend listens for this to reset transient UI state (e.g.
/// the Settings panel) so reopening starts from the list view.
pub const POPUP_HIDDEN_EVENT: &str = "popup-hidden";

// macOS uses a template-mode monochrome icon so the system can tint it based
// on menubar state (dark/light/focus). Windows and Linux taskbars don't
// support template mode — they expect a normal colored icon. Shipping two
// assets and picking at build time keeps the tray recognizable on each OS.
#[cfg(target_os = "macos")]
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");

const TRAY_ID: &str = "main";

#[tauri::command]
pub fn set_tray_badge(count: u32, has_unread: bool, app: tauri::AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    // macOS menubar: adjacent text next to the icon.
    #[cfg(target_os = "macos")]
    {
        let title = if count == 0 {
            None
        } else if has_unread {
            // Red-dot emoji prefix stands in for colored text: macOS tray
            // titles are plain strings (no NSAttributedString via tauri),
            // so we flag "unread exists" with a visible glyph instead.
            Some(format!(" 🔴 {count}"))
        } else {
            Some(format!(" {count}"))
        };
        eprintln!("[eir] set_tray_badge count={count} has_unread={has_unread} title={title:?}");
        let _ = tray.set_title(title);
    }

    // Windows / Linux: no adjacent-text slot on the tray icon, so surface the
    // count + unread state through the hover tooltip instead.
    #[cfg(not(target_os = "macos"))]
    {
        let tooltip = if count == 0 {
            "eir".to_string()
        } else if has_unread {
            format!("eir — {count} (unread)")
        } else {
            format!("eir — {count}")
        };
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}

/// Toggle the popup window. If visible, hide it. If hidden, position it
/// near the tray icon and show it.
///
/// All NSWindow-mutating work is dispatched onto the main thread. The tray
/// click path is already on the main thread so this is a no-op hop, but
/// the global-shortcut handler runs on the shortcut listener thread and
/// must cross over — calling `set_position` / `set_size` / `show` directly
/// from that thread risks each call being queued onto the main thread
/// individually, which can reorder operations. Wrapping the whole sequence
/// in a single main-thread closure makes the ordering deterministic.
pub fn toggle_popup(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        toggle_popup_on_main(&app_handle);
    });
}

fn toggle_popup_on_main(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        let _ = app.emit(POPUP_HIDDEN_EVENT, ());
        return;
    }
    position_near_tray(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Pick the monitor whose bounds contain the given point, falling back to
/// the window's current monitor, then primary, then the first available.
///
/// `window.current_monitor()` alone is unreliable in multi-monitor setups:
/// it returns whichever monitor the window was last shown on, so if the
/// popup was previously open on the main monitor and the user clicks the
/// tray on a secondary monitor, the returned bounds describe the *wrong*
/// screen and the popup snaps to the edge of the main monitor instead of
/// appearing under the tray icon the user actually clicked.
fn monitor_containing(window: &tauri::WebviewWindow, x: i32, y: i32) -> Option<tauri::Monitor> {
    let monitors = window.available_monitors().ok().unwrap_or_default();
    if let Some(m) = monitors.iter().find(|m| {
        let p = m.position();
        let s = m.size();
        let left = p.x;
        let right = p.x + s.width as i32;
        let top = p.y;
        let bottom = p.y + s.height as i32;
        x >= left && x < right && y >= top && y < bottom
    }) {
        return Some(m.clone());
    }
    window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| monitors.into_iter().next())
}

fn position_near_tray(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let Ok(Some(rect)) = tray.rect() else {
        return;
    };

    let (tray_x, tray_y, tray_w, tray_h) = match (rect.position, rect.size) {
        (tauri::Position::Physical(p), tauri::Size::Physical(s)) => {
            (p.x, p.y, s.width as i32, s.height as i32)
        }
        (tauri::Position::Logical(p), tauri::Size::Logical(s)) => {
            (p.x as i32, p.y as i32, s.width as i32, s.height as i32)
        }
        _ => return,
    };

    let win_size = window.outer_size().unwrap_or_default();
    let popup_width = win_size.width as i32;
    let current_popup_height = win_size.height as i32;

    // Pick the monitor that physically contains the tray icon — not whichever
    // monitor the window happens to be sitting on. Matches the user's click.
    let tray_center_x = tray_x + tray_w / 2;
    let tray_center_y = tray_y + tray_h / 2;
    let monitor = monitor_containing(window, tray_center_x, tray_center_y);
    let (monitor_top, monitor_bottom, monitor_left, monitor_right) = match monitor.as_ref() {
        Some(m) => {
            let pos = m.position();
            let size = m.size();
            (
                pos.y,
                pos.y + size.height as i32,
                pos.x,
                pos.x + size.width as i32,
            )
        }
        None => (0, i32::MAX, 0, i32::MAX),
    };

    // Decide above vs below: tray closer to the top of the monitor → drop the
    // popup below it (macOS menubar). Tray closer to the bottom → raise the
    // popup above it (Windows taskbar / KDE default).
    let monitor_mid_y = monitor_top + (monitor_bottom - monitor_top) / 2;
    let open_below = tray_center_y < monitor_mid_y;

    // Compute the target size first — no side effects yet. On macOS we take
    // advantage of having the tray at the top to grow the popup to fill most
    // of the screen height. On Windows/Linux the tray isn't consistently at
    // one edge, so we stick with the current size.
    #[cfg(target_os = "macos")]
    let target_height = match monitor.as_ref() {
        Some(m) => {
            let dock_margin = (80.0 * m.scale_factor()) as i32;
            let bottom = monitor_top + m.size().height as i32;
            let top_if_below = tray_y + tray_h + 8;
            (bottom - top_if_below - dock_margin).max(400)
        }
        None => current_popup_height,
    };
    #[cfg(not(target_os = "macos"))]
    let target_height = current_popup_height;

    let popup_top = if open_below {
        tray_y + tray_h + 8
    } else {
        // Subtract the target height; fall back to tray_y - 8 if we can't
        // determine a reasonable anchor.
        (tray_y - target_height - 8).max(monitor_top + 8)
    };

    let mut x = tray_center_x - popup_width / 2;
    // Keep the window fully on the target monitor.
    let max_x = monitor_right - popup_width - 8;
    let min_x = monitor_left + 8;
    if x > max_x {
        x = max_x;
    }
    if x < min_x {
        x = min_x;
    }

    // Apply in an order that avoids a cross-monitor flash. The window is
    // still hidden here, but on macOS `set_size` resolves against whichever
    // screen the NSWindow is currently anchored to — typically the monitor
    // where it was last shown. If we size first, show() briefly renders the
    // resized window on the *old* monitor before our position update moves
    // it, which the user sees as a flicker. Setting the position first
    // anchors the window to the target monitor so the resize happens in the
    // right context; a second set_position call after set_size covers any
    // implicit origin adjustment macOS may perform when the frame grows.
    let _ = window.set_position(PhysicalPosition { x, y: popup_top });
    let _ = window.set_size(tauri::PhysicalSize {
        width: popup_width as u32,
        height: target_height as u32,
    });
    let _ = window.set_position(PhysicalPosition { x, y: popup_top });
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit eir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

    let tray_icon = Image::from_bytes(TRAY_ICON_BYTES)?;
    let builder = TrayIconBuilder::with_id(TRAY_ID).icon(tray_icon);

    // Template mode is a macOS concept (NSImage tinting to match menubar
    // appearance). Other platforms render the icon as-is.
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);

    builder
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popup(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
