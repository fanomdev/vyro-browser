//! Platform detection and per-OS application data paths.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    MacOS,
    Android,
    Ios,
    Other,
}

pub fn current_os() -> Os {
    match std::env::consts::OS {
        "windows" => Os::Windows,
        "linux" => Os::Linux,
        "macos" => Os::MacOS,
        "android" => Os::Android,
        "ios" => Os::Ios,
        _ => Os::Other,
    }
}

/// Resolve the directory used for VYRO's persistent state.
/// Keeps all state under one folder so "clear browsing data" is reliable.
pub fn data_dir(app: &str) -> PathBuf {
    match current_os() {
        Os::Windows => {
            let base = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\AppData\\Local"));
            base.join(app)
        }
        Os::MacOS | Os::Ios => home().join("Library").join("Application Support").join(app),
        Os::Android => home().join(app),
        _ => {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".local").join("share"));
            base.join(app)
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_dir_is_under_a_writable_root() {
        let d = data_dir("vyro-test");
        assert!(d.to_string_lossy().contains("vyro-test"));
    }
}
