//! User settings with sensible privacy-first defaults and JSON persistence.

use crate::engine::SearchEngine;
use crate::storage::JsonStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Dark
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    // Appearance
    pub theme: Theme,
    pub accent_color: String,
    pub homepage: String,
    pub new_tab_background: String,
    pub show_bookmarks_bar: bool,
    pub sidebar_visible: bool,
    pub toolbar: Vec<String>,
    // Search & navigation
    pub search_engine: SearchEngine,
    pub show_home_button: bool,
    pub restore_session: bool,
    pub zoom_level: f32,
    // Privacy & protection
    pub blocker_enabled: bool,
    pub tracking_protection: bool,
    pub https_upgrade: bool,
    pub fingerprinting_protection: bool,
    pub safe_browsing: bool,
    pub block_third_party_cookies: bool,
    pub clear_on_exit: bool,
    /// Absolute path where downloaded files are saved.
    #[serde(default = "default_download_dir")]
    pub download_dir: String,
    // VPN
    pub vpn_provider_id: Option<String>,
    pub vpn_kill_switch: bool,
    pub vpn_dns_leak_protection: bool,
    pub vpn_auto_reconnect: bool,
}

fn default_download_dir() -> String {
    crate::platform::downloads_dir().to_string_lossy().into_owned()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            accent_color: "#7C5CFF".into(),
            homepage: "https://duckduckgo.com".into(),
            new_tab_background: "aurora".into(),
            show_bookmarks_bar: true,
            sidebar_visible: false,
            toolbar: vec![
                "back".into(),
                "forward".into(),
                "reload".into(),
                "home".into(),
                "address".into(),
                "shield".into(),
                "bookmark".into(),
                "extensions".into(),
                "downloads".into(),
                "privacy".into(),
                "menu".into(),
            ],
            search_engine: SearchEngine::DuckDuckGo,
            show_home_button: true,
            restore_session: true,
            zoom_level: 1.0,
            blocker_enabled: true,
            tracking_protection: true,
            https_upgrade: true,
            fingerprinting_protection: false,
            safe_browsing: true,
            block_third_party_cookies: true,
            clear_on_exit: false,
            download_dir: default_download_dir(),
            vpn_provider_id: None,
            vpn_kill_switch: false,
            vpn_dns_leak_protection: true,
            vpn_auto_reconnect: true,
        }
    }
}

impl Settings {
    pub fn load_or_default(store: &JsonStore<Self>) -> Self {
        store.load_or(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_privacy_first() {
        let s = Settings::default();
        assert_eq!(s.theme, Theme::Dark);
        assert!(s.blocker_enabled && s.tracking_protection && s.https_upgrade);
        assert!(s.block_third_party_cookies);
    }

    #[test]
    fn serde_roundtrip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
