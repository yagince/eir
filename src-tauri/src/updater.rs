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
///
/// The `open` must not run while this process is still alive: Launch Services
/// starts the new instance in a few hundred ms, before the old one has
/// finished exiting, and the two then race over process-wide OS resources —
/// most fatally the global shortcut, whose cross-process duplicate
/// registration fails (`eventHotKeyExistsErr`). A detached shell (orphaned to
/// launchd, so it survives our exit) delays the launch until the old instance
/// is gone. The bundle path is passed as `$0` rather than interpolated so
/// paths with spaces or quotes can't break the -c string.
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

        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(r#"sleep 1; exec /usr/bin/open -n "$0""#)
            .arg(bundle)
            .spawn()
            .map_err(|e| e.to_string())?;

        crate::diagnostics::log(&format!(
            "relaunch: delayed open -n scheduled for {}; exiting old instance",
            bundle.display()
        ));
        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.restart();
    }
}
