use tauri::AppHandle;

/// Restart eir after an in-place updater install.
///
/// `tauri_plugin_process::relaunch` (and `AppHandle::restart`) spawn
/// `current_exe()` directly, which on macOS bypasses Launch Services. For an
/// `ActivationPolicy::Accessory` menubar app — and especially right after the
/// updater swapped the .app bundle on disk — that spawn silently fails: the
/// old process exits and nothing shows up in the menubar. Going through
/// `/usr/bin/open -n` re-enters Launch Services and reliably brings the new
/// instance up.
#[tauri::command]
pub async fn relaunch_app(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // exe = .../eir.app/Contents/MacOS/eir → three parents up is the bundle.
        let bundle = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or_else(|| "could not resolve .app bundle path".to_string())?;

        std::process::Command::new("/usr/bin/open")
            .arg("-n")
            .arg(bundle)
            .spawn()
            .map_err(|e| e.to_string())?;

        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.restart();
    }
}
