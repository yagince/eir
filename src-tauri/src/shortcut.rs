use std::path::PathBuf;

use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+E";

fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/eir/shortcut"))
}

pub fn load_shortcut_string() -> String {
    if let Some(p) = config_path() {
        if let Ok(s) = std::fs::read_to_string(p) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    DEFAULT_SHORTCUT.to_string()
}

fn save_shortcut_string(s: &str) {
    let Some(p) = config_path() else {
        return;
    };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(p, s);
}

pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    s.parse::<Shortcut>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_toggle_shortcut() -> String {
    load_shortcut_string()
}

#[tauri::command]
pub fn set_toggle_shortcut(shortcut: String, app: tauri::AppHandle) -> Result<(), String> {
    let parsed = parse_shortcut(&shortcut)?;
    let gs = app.global_shortcut();
    // Clear any previously-registered toggle binding before installing the
    // new one, so rebinding cleanly replaces rather than stacks.
    let _ = gs.unregister_all();
    gs.register(parsed).map_err(|e| e.to_string())?;
    save_shortcut_string(&shortcut);
    Ok(())
}
