use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, PhysicalPosition,
};

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
pub fn toggle_popup(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    position_near_tray(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
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
    let popup_height = win_size.height as i32;

    let monitor = window.current_monitor().ok().flatten();
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
    let tray_center_y = tray_y + tray_h / 2;
    let monitor_mid_y = monitor_top + (monitor_bottom - monitor_top) / 2;
    let open_below = tray_center_y < monitor_mid_y;

    // On macOS we take advantage of having the tray at the top to grow the
    // popup to fill most of the screen height. On Windows/Linux the tray
    // isn't consistently at one edge, so we stick with the configured size.
    #[cfg(target_os = "macos")]
    let popup_height = {
        if let Some(m) = monitor.as_ref() {
            let dock_margin = (80.0 * m.scale_factor()) as i32;
            let bottom = monitor_top + m.size().height as i32;
            let popup_top = tray_y + tray_h + 8;
            let target_height = (bottom - popup_top - dock_margin).max(400);
            let _ = window.set_size(tauri::PhysicalSize {
                width: popup_width as u32,
                height: target_height as u32,
            });
            target_height
        } else {
            popup_height
        }
    };

    let popup_top = if open_below {
        tray_y + tray_h + 8
    } else {
        // Subtract the (possibly resized) height; fall back to tray_y - 8 if we
        // can't determine a reasonable anchor.
        (tray_y - popup_height - 8).max(monitor_top + 8)
    };

    let tray_center_x = tray_x + tray_w / 2;
    let mut x = tray_center_x - popup_width / 2;
    // Keep the window fully on the current monitor.
    let max_x = monitor_right - popup_width - 8;
    let min_x = monitor_left + 8;
    if x > max_x {
        x = max_x;
    }
    if x < min_x {
        x = min_x;
    }

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
