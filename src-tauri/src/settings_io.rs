use std::path::PathBuf;

#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, contents).map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(PathBuf::from(&path)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// These two commands take an absolute path chosen by the frontend and
    /// never consult `HOME`, so — unlike `auth`, `shortcut`, `snooze` and
    /// `diagnostics` — they need no environment manipulation and can't reach
    /// the developer's real `~/.config/eir/`. A per-test scratch dir is enough;
    /// the name only has to be unique so parallel tests can't collide.
    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "eir-settings-io-test-{tag}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        /// A path *inside* the scratch dir, in the `String` form the commands take.
        fn path(&self, rel: &str) -> String {
            self.dir.join(rel).to_string_lossy().into_owned()
        }

        fn dir_path(&self) -> String {
            self.dir.to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Every case must come back as an error with a message the frontend can
    /// surface — an empty string would show the user a blank alert.
    fn assert_err_with_message(label: &str, result: Result<String, String>) {
        match result {
            Ok(v) => panic!("{label} should have failed, got Ok({v:?})"),
            Err(msg) => assert!(!msg.is_empty(), "empty error message for {label}"),
        }
    }

    #[test]
    fn write_creates_missing_parent_dirs_and_returns_the_path() {
        let s = Scratch::new("parents");
        // The frontend hands us `<config>/eir/settings.json` on a fresh machine
        // where nothing in that chain exists yet.
        let target = s.path("nested/deeper/settings.json");

        let returned = write_text_file(target.clone(), "{}".to_string()).unwrap();

        assert_eq!(returned, target, "write should report the path it wrote");
        assert_eq!(read_text_file(target).unwrap(), "{}");
    }

    #[test]
    fn round_trip_preserves_payloads_byte_for_byte() {
        let s = Scratch::new("payloads");
        let target = s.path("settings.json");
        // Deliberately content-agnostic: this is the transport for settings
        // JSON, not a validator, so even unparseable or non-ASCII text has to
        // come back unchanged — including the empty file, which must read as
        // `Ok("")` rather than an error.
        let payloads = [
            "{\"activeTab\":\"mine\",\"refreshMs\":60000}",
            "{ not json at all",
            "{\"repo\":\"オーナー/リポジトリ\"}\n\ntrailing\n",
            "",
        ];

        for payload in payloads {
            write_text_file(target.clone(), payload.to_string()).unwrap();
            assert_eq!(
                read_text_file(target.clone()).unwrap(),
                payload,
                "round trip of {payload:?}"
            );
        }
    }

    #[test]
    fn write_truncates_a_longer_previous_value() {
        let s = Scratch::new("truncate");
        let target = s.path("settings.json");
        write_text_file(target.clone(), "{\"a\":1,\"b\":2,\"c\":3}".to_string()).unwrap();

        // Leftover bytes from the longer value would make the file invalid JSON.
        write_text_file(target.clone(), "{}".to_string()).unwrap();

        assert_eq!(read_text_file(target).unwrap(), "{}");
    }

    #[test]
    fn read_reports_an_error_for_unreadable_paths() {
        let s = Scratch::new("read-errors");
        std::fs::write(s.dir.join("binary.bin"), [0xff, 0xfe, 0x00]).unwrap();
        let cases = [
            ("a missing file", s.path("does-not-exist.json")),
            ("a directory", s.dir_path()),
            // `read_to_string` rejects invalid UTF-8 rather than lossily
            // mangling it, so a corrupted settings file is reported, not parsed.
            ("a non-UTF-8 file", s.path("binary.bin")),
        ];

        for (label, path) in cases {
            assert_err_with_message(label, read_text_file(path));
        }
    }

    #[test]
    fn write_reports_an_error_instead_of_panicking_on_unwritable_paths() {
        let s = Scratch::new("write-errors");
        std::fs::write(s.dir.join("blocker"), "not a directory").unwrap();
        let cases = [
            // `create_dir_all` can't turn an existing file into a parent dir.
            (
                "a parent component that is a file",
                s.path("blocker/settings.json"),
            ),
            ("an existing directory", s.dir_path()),
        ];

        for (label, path) in cases {
            assert_err_with_message(label, write_text_file(path, "{}".to_string()));
        }
    }
}
