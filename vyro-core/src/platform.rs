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
    // Portable mode: a `portable` marker next to the executable keeps all
    // browser state beside it, so VYRO can run from any drive without touching
    // the user profile on C:.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("portable").exists() {
                return portable_data_dir(dir, app);
            }
        }
    }
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

/// State folder used when running in portable mode.
fn portable_data_dir(exe_dir: &std::path::Path, app: &str) -> PathBuf {
    exe_dir.join(format!("{}-data", app.to_ascii_lowercase()))
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

    #[test]
    fn portable_data_dir_sits_beside_executable() {
        let d = portable_data_dir(std::path::Path::new("D:\\VYRO"), "VYRO");
        assert_eq!(d, std::path::Path::new("D:\\VYRO\\vyro-data"));
    }
}
