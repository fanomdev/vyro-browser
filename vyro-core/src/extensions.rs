//! Extension inventory and management with an explicit, honest capability report.
//!
//! The webview engines used by the desktop/mobile shells (WebView2, WKWebView,
//! WebKitGTK, Android WebView) do **not** expose the Chromium `chrome.*`
//! extension API. VYRO therefore provides a real management/inventory layer for
//! unpacked extensions (parsing `manifest.json`), plus a first-party companion
//! bridge, but does NOT claim Chrome Web Store compatibility.
//!
//! For genuine Chrome Web Store extension support, the engine must be a
//! Chromium fork with the extension service enabled — see docs/LIMITATIONS.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub manifest_version: u32,
    pub enabled: bool,
    pub permissions: Vec<String>,
    pub background_scripts: usize,
    pub content_scripts: usize,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionError {
    InvalidManifest(String),
    Duplicate(String),
    NotFound(String),
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionError::InvalidManifest(e) => write!(f, "Invalid extension manifest: {}", e),
            ExtensionError::Duplicate(id) => write!(f, "Extension '{}' is already installed.", id),
            ExtensionError::NotFound(id) => write!(f, "Extension '{}' is not installed.", id),
        }
    }
}
impl std::error::Error for ExtensionError {}

/// Parse an unpacked Chrome-style `manifest.json`.
pub fn parse_manifest(json: &str, path: &str, id: &str) -> Result<ExtensionRecord, ExtensionError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ExtensionError::InvalidManifest(e.to_string()))?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExtensionError::InvalidManifest("missing 'name'".into()))?
        .to_string();
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let manifest_version = value
        .get("manifest_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as u32;
    let permissions: Vec<String> = value
        .get("permissions")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let background_scripts = value
        .get("background")
        .and_then(|b| b.get("scripts"))
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .or_else(|| {
            value
                .get("background")
                .and_then(|b| b.get("service_worker"))
                .and_then(|v| v.as_str())
                .map(|_| 1)
        })
        .unwrap_or(0);
    let content_scripts = value
        .get("content_scripts")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(ExtensionRecord {
        id: id.to_string(),
        name,
        version,
        description,
        manifest_version,
        enabled: false,
        permissions,
        background_scripts,
        content_scripts,
        path: path.to_string(),
    })
}

#[derive(Debug, Default)]
pub struct ExtensionManager {
    items: Vec<ExtensionRecord>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(&mut self, record: ExtensionRecord) -> Result<(), ExtensionError> {
        if self.items.iter().any(|e| e.id == record.id) {
            return Err(ExtensionError::Duplicate(record.id));
        }
        self.items.push(record);
        Ok(())
    }

    pub fn install_from_manifest(
        &mut self,
        json: &str,
        path: &str,
        id: &str,
    ) -> Result<(), ExtensionError> {
        let record = parse_manifest(json, path, id)?;
        self.install(record)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), ExtensionError> {
        let before = self.items.len();
        self.items.retain(|e| e.id != id);
        if self.items.len() == before {
            Err(ExtensionError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), ExtensionError> {
        let ext = self
            .items
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?;
        ext.enabled = enabled;
        Ok(())
    }

    pub fn list(&self) -> &[ExtensionRecord] {
        &self.items
    }

    /// What this engine can actually do — shown verbatim in the UI.
    pub fn support_report() -> SupportReport {
        SupportReport {
            chrome_web_store_supported: false,
            engine: "System WebView (WebView2 / WKWebView / WebKitGTK / Android WebView)".into(),
            supported: vec![
                "Install/remove/enable/disable unpacked extensions".into(),
                "Read and display manifest permissions".into(),
                "First-party companion bridge (postMessage)".into(),
            ],
            unsupported: vec![
                "chrome.* extension APIs (tabs, webRequest, storage, runtime)".into(),
                "Chrome Web Store installation and updates".into(),
                "Background service workers and content script injection".into(),
            ],
            note: "Genuine Chrome extension support requires a Chromium fork with the \
                   extensions service. VYRO reports this honestly rather than faking listings."
                .into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportReport {
    pub chrome_web_store_supported: bool,
    pub engine: String,
    pub supported: Vec<String>,
    pub unsupported: Vec<String>,
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "manifest_version": 3,
        "name": "VYRO Companion",
        "version": "1.2.0",
        "description": "First-party companion",
        "permissions": ["storage", "activeTab"],
        "background": { "service_worker": "sw.js" },
        "content_scripts": [ { "matches": ["<all_urls>"], "js": ["content.js"] } ]
    }"#;

    #[test]
    fn parses_manifest_fields() {
        let r = parse_manifest(MANIFEST, "/ext", "companion").unwrap();
        assert_eq!(r.name, "VYRO Companion");
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.manifest_version, 3);
        assert_eq!(r.permissions.len(), 2);
        assert_eq!(r.background_scripts, 1);
        assert_eq!(r.content_scripts, 1);
    }

    #[test]
    fn manager_lifecycle() {
        let mut m = ExtensionManager::new();
        m.install_from_manifest(MANIFEST, "/ext", "x").unwrap();
        assert_eq!(m.list().len(), 1);
        assert!(m.install_from_manifest(MANIFEST, "/ext", "x").is_err());
        m.set_enabled("x", true).unwrap();
        assert!(m.list()[0].enabled);
        m.remove("x").unwrap();
        assert!(m.list().is_empty());
    }

    #[test]
    fn report_is_honest_about_limitations() {
        let r = ExtensionManager::support_report();
        assert!(!r.chrome_web_store_supported);
        assert!(!r.unsupported.is_empty());
    }
}
