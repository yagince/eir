use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, PhysicalPosition,
};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");
const TRAY_ID: &str = "main";

#[tauri::command]
pub fn set_tray_badge(count: u32, app: tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let title = if count == 0 {
            None
        } else {
            Some(format!(" {count}"))
        };
        let _ = tray.set_title(title);
    }
}

/// Toggle the popup window. If visible, hide it. If hidden, position it
/// under the tray icon (centered) and show it.
pub fn toggle_popup(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    position_under_tray(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
}

fn position_under_tray(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
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
    let tray_center_x = tray_x + tray_w / 2;
    let x = tray_center_x - (win_size.width as i32) / 2;
    let y = tray_y + tray_h + 8;
    let _ = window.set_position(PhysicalPosition { x, y });
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit eir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

    let tray_icon = Image::from_bytes(TRAY_ICON_BYTES)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon)
        .icon_as_template(true)
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
