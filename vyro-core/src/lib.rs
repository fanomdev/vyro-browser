//! # vyro-core
//!
//! Platform-independent core logic for VYRO Browser.
//!
//! This crate contains the real, testable browser logic: navigation intent
//! resolution, ad/tracker filtering, tab and session state, bookmarks, history,
//! downloads, privacy controls, a provider-agnostic VPN integration layer, and
//! an extension management model.
//!
//! It deliberately has **no** UI or webview dependency so every subsystem can be
//! compiled and unit-tested on its own. The Tauri shell (`src-tauri`) wires these
//! modules to the browser UI.

pub mod adblock;
pub mod bookmarks;
pub mod companions;
pub mod downloads;
pub mod engine;
pub mod extensions;
pub mod history;
pub mod platform;
pub mod privacy;
pub mod safe_browsing;
pub mod settings;
pub mod storage;
pub mod tabs;
pub mod vault;
pub mod vpn;
pub mod vpn_gate;

/// VYRO product constants — single source of truth for branding.
pub const APP_NAME: &str = "VYRO";
pub const PRODUCT_NAME: &str = "VYRO Browser";
pub const TAGLINE: &str = "Browse Beyond Limits.";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
