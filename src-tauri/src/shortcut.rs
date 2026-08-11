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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    // `CmdOrCtrl` (and its aliases) is the one modifier whose meaning is
    // platform-dependent: Command on macOS, Control everywhere else.
    #[cfg(target_os = "macos")]
    const CMD_OR_CTRL: Modifiers = Modifiers::SUPER;
    #[cfg(not(target_os = "macos"))]
    const CMD_OR_CTRL: Modifiers = Modifiers::CONTROL;

    fn assert_parses(cases: &[(&str, Modifiers, Code)]) {
        for (input, mods, code) in cases {
            let parsed =
                parse_shortcut(input).unwrap_or_else(|e| panic!("{input:?} should parse: {e}"));
            assert_eq!(parsed.mods, *mods, "modifiers for {input:?}");
            assert_eq!(parsed.key, *code, "key for {input:?}");
        }
    }

    fn assert_rejected(cases: &[&str]) {
        for input in cases {
            match parse_shortcut(input) {
                Ok(s) => panic!(
                    "{input:?} should be rejected but parsed as {}",
                    s.into_string()
                ),
                // lib.rs logs this message to the diagnostics file when startup
                // registration fails, so an empty one would be useless there.
                Err(msg) => assert!(!msg.is_empty(), "empty error message for {input:?}"),
            }
        }
    }

    // ---- parsing -----------------------------------------------------------

    #[test]
    fn default_shortcut_constant_parses() {
        // lib.rs falls back to DEFAULT_SHORTCUT when the stored string is
        // unparseable, so if this constant ever stops parsing the app silently
        // launches with no global shortcut at all.
        assert_eq!(DEFAULT_SHORTCUT, "Ctrl+Shift+E");
        assert_parses(&[(
            DEFAULT_SHORTCUT,
            Modifiers::CONTROL | Modifiers::SHIFT,
            Code::KeyE,
        )]);
    }

    #[test]
    fn parses_every_modifier_alias() {
        assert_parses(&[
            ("Ctrl+E", Modifiers::CONTROL, Code::KeyE),
            ("Control+E", Modifiers::CONTROL, Code::KeyE),
            ("Shift+E", Modifiers::SHIFT, Code::KeyE),
            ("Alt+E", Modifiers::ALT, Code::KeyE),
            ("Option+E", Modifiers::ALT, Code::KeyE),
            ("Cmd+E", Modifiers::SUPER, Code::KeyE),
            ("Command+E", Modifiers::SUPER, Code::KeyE),
            ("Super+E", Modifiers::SUPER, Code::KeyE),
        ]);
    }

    #[test]
    fn parses_cmd_or_ctrl_aliases_to_the_platform_modifier() {
        assert_parses(&[
            ("CmdOrCtrl+E", CMD_OR_CTRL, Code::KeyE),
            ("CmdOrControl+E", CMD_OR_CTRL, Code::KeyE),
            ("CommandOrCtrl+E", CMD_OR_CTRL, Code::KeyE),
            ("CommandOrControl+E", CMD_OR_CTRL, Code::KeyE),
        ]);
    }

    #[test]
    fn parses_modifier_combinations() {
        assert_parses(&[
            (
                "Ctrl+Shift+E",
                Modifiers::CONTROL | Modifiers::SHIFT,
                Code::KeyE,
            ),
            ("Cmd+Alt+E", Modifiers::SUPER | Modifiers::ALT, Code::KeyE),
            (
                "Ctrl+Alt+Shift+E",
                Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
                Code::KeyE,
            ),
            (
                "Cmd+Ctrl+Alt+Shift+E",
                Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
                Code::KeyE,
            ),
        ]);
    }

    #[test]
    fn modifier_order_does_not_change_the_result() {
        let a = parse_shortcut("Ctrl+Shift+E").unwrap();
        let b = parse_shortcut("Shift+Ctrl+E").unwrap();
        assert_eq!(a, b);
        // The id is what the OS registration is keyed on, so equal shortcuts
        // must also produce equal ids.
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn modifier_and_key_names_are_case_insensitive() {
        let expected = parse_shortcut(DEFAULT_SHORTCUT).unwrap();
        for input in ["ctrl+shift+e", "CTRL+SHIFT+E", "CtRl+ShIfT+e"] {
            assert_eq!(
                parse_shortcut(input).unwrap(),
                expected,
                "case for {input:?}"
            );
        }
    }

    #[test]
    fn parses_letters_digits_and_function_keys() {
        assert_parses(&[
            ("Ctrl+A", Modifiers::CONTROL, Code::KeyA),
            ("Ctrl+KeyA", Modifiers::CONTROL, Code::KeyA),
            ("Ctrl+Z", Modifiers::CONTROL, Code::KeyZ),
            ("Ctrl+5", Modifiers::CONTROL, Code::Digit5),
            ("Ctrl+Digit5", Modifiers::CONTROL, Code::Digit5),
            ("Ctrl+F1", Modifiers::CONTROL, Code::F1),
            ("Ctrl+F12", Modifiers::CONTROL, Code::F12),
        ]);
    }

    #[test]
    fn parses_named_and_punctuation_keys() {
        assert_parses(&[
            ("Ctrl+Space", Modifiers::CONTROL, Code::Space),
            ("Ctrl+Enter", Modifiers::CONTROL, Code::Enter),
            ("Ctrl+Escape", Modifiers::CONTROL, Code::Escape),
            ("Ctrl+Esc", Modifiers::CONTROL, Code::Escape),
            ("Ctrl+ArrowUp", Modifiers::CONTROL, Code::ArrowUp),
            ("Ctrl+Up", Modifiers::CONTROL, Code::ArrowUp),
            ("Ctrl+/", Modifiers::CONTROL, Code::Slash),
            ("Ctrl+,", Modifiers::CONTROL, Code::Comma),
        ]);
    }

    #[test]
    fn accepts_a_bare_key_with_no_modifier() {
        // Not a typo: a modifier-less binding parses fine. Rejecting it (as
        // "too easy to trigger") would have to happen in the settings UI, not
        // here, so this pins where that responsibility currently sits.
        assert_parses(&[
            ("E", Modifiers::empty(), Code::KeyE),
            ("F5", Modifiers::empty(), Code::F5),
        ]);
    }

    #[test]
    fn tolerates_spaces_around_tokens() {
        let expected = parse_shortcut(DEFAULT_SHORTCUT).unwrap();
        for input in ["Ctrl + Shift + E", " Ctrl+Shift+E ", "Ctrl+ Shift +E"] {
            assert_eq!(
                parse_shortcut(input).unwrap(),
                expected,
                "spacing for {input:?}"
            );
        }
    }

    #[test]
    fn rejects_input_with_no_main_key() {
        assert_rejected(&[
            "",
            "Shift",
            "Ctrl+Shift",
            "Ctrl+Alt",
            "Cmd+Ctrl+Alt+Shift",
            // Trailing/leading/doubled separators leave an empty token.
            "Ctrl+",
            "+E",
            "Ctrl++E",
            "+",
        ]);
    }

    #[test]
    fn rejects_unknown_modifier_and_key_names() {
        assert_rejected(&[
            // Unknown modifier names fall through to the key parser and fail
            // there — including "Meta", which the Shortcut type understands
            // programmatically but the string format does not accept.
            "Meta+E",
            "Hyper+E",
            "Mod+E",
            "Win+E",
            "Ctrl+Shift+NotAKey",
            "Ctrl+F25",
            "Ctrl+KeyEE",
        ]);
    }

    #[test]
    fn rejects_missing_separators_and_wrong_order() {
        assert_rejected(&[
            "CtrlShiftE",
            "Ctrl Shift E",
            // Modifiers must come first, and only one main key is allowed.
            "E+Ctrl",
            "Ctrl+E+Shift",
            "Ctrl+A+B",
            // A lone key is *not* trimmed (only multi-token input is), so a
            // padded single token is rejected. load_shortcut_string() trims
            // the file contents before parsing, which is what hides this.
            " E ",
        ]);
    }

    // ---- persistence -------------------------------------------------------

    use crate::test_env::HOME_LOCK;

    /// Points `$HOME` (which `config_path()` reads, with no injection seam) at
    /// a fresh temp dir for the duration of one test, serialising against the
    /// other tests that do the same and restoring the previous value on drop.
    struct HomeGuard {
        _lock: MutexGuard<'static, ()>,
        prev: Option<OsString>,
        dir: PathBuf,
    }

    impl HomeGuard {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let lock = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "eir-shortcut-test-{tag}-{}-{}-{nanos}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let prev = std::env::var_os("HOME");
            std::env::set_var("HOME", &dir);
            Self {
                _lock: lock,
                prev,
                dir,
            }
        }

        fn shortcut_path(&self) -> PathBuf {
            self.dir.join(".config/eir/shortcut")
        }

        fn write_stored(&self, contents: &str) {
            let p = self.shortcut_path();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
        }

        fn read_stored(&self) -> String {
            std::fs::read_to_string(self.shortcut_path()).unwrap()
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn load_shortcut_string_returns_the_stored_value() {
        let home = HomeGuard::new("load-stored");
        home.write_stored("Cmd+Shift+K");
        assert_eq!(load_shortcut_string(), "Cmd+Shift+K");
    }

    #[test]
    fn load_shortcut_string_trims_surrounding_whitespace() {
        let home = HomeGuard::new("load-trim");
        // A hand-edited config file picks up a trailing newline; without the
        // trim the value would differ from what the settings UI round-trips.
        home.write_stored("\tCtrl+Alt+K \n");
        assert_eq!(load_shortcut_string(), "Ctrl+Alt+K");
    }

    #[test]
    fn load_shortcut_string_falls_back_when_file_is_missing() {
        let _home = HomeGuard::new("load-missing");
        assert_eq!(load_shortcut_string(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn load_shortcut_string_falls_back_when_file_is_blank() {
        let home = HomeGuard::new("load-blank");
        // A truncated/empty file must not yield "", which parses to an error
        // and would leave the app with no shortcut at all.
        home.write_stored("  \n\t");
        assert_eq!(load_shortcut_string(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn load_shortcut_string_falls_back_when_home_is_unset() {
        let _home = HomeGuard::new("load-no-home");
        std::env::remove_var("HOME");
        assert_eq!(load_shortcut_string(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn get_toggle_shortcut_command_returns_the_stored_value() {
        let home = HomeGuard::new("cmd-get");
        home.write_stored("Cmd+Shift+J\n");
        assert_eq!(get_toggle_shortcut(), "Cmd+Shift+J");
    }

    #[test]
    fn save_shortcut_string_creates_the_config_directory() {
        let home = HomeGuard::new("save-mkdir");
        // The temp HOME has no .config/eir yet, which is the first-run case.
        save_shortcut_string("Cmd+Shift+L");
        assert_eq!(home.read_stored(), "Cmd+Shift+L");
    }

    #[test]
    fn save_shortcut_string_replaces_a_longer_previous_value() {
        let home = HomeGuard::new("save-overwrite");
        home.write_stored("Ctrl+Alt+Shift+F12");
        save_shortcut_string("Ctrl+E");
        assert_eq!(home.read_stored(), "Ctrl+E");
    }

    #[test]
    fn save_then_load_round_trips_the_value() {
        let _home = HomeGuard::new("round-trip");
        save_shortcut_string("Alt+Shift+Space");
        let loaded = load_shortcut_string();
        assert_eq!(loaded, "Alt+Shift+Space");
        assert!(parse_shortcut(&loaded).is_ok());
    }
}
