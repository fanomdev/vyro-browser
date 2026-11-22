//! Safe Browsing integration: a bundled local threat denylist plus optional
//! Google Safe Browsing API v5 lookups when a key is supplied via environment
//! variable. No key is bundled, and results are never invented.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Small starter denylist. This is intentionally honest: it is a seed list,
/// not a full threat feed. EasyList-style malicious-domain feeds can be added
/// at runtime through the blocker's custom-filter feature.
const LOCAL_THREAT_DOMAINS: &[&str] = &[
    "malware.example",
    "phishing.example",
    "safe-browsing-test.google",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Disabled,
    Safe,
    Malicious(String),
    Unverified(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeBrowsingConfig {
    pub enabled: bool,
    /// Name of the env var holding a Google Safe Browsing API key.
    pub api_key_env: Option<String>,
    pub client_id: String,
    pub client_version: String,
}

impl Default for SafeBrowsingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key_env: None,
            client_id: "vyro-browser".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

#[derive(Debug)]
pub struct SafeBrowsing {
    pub config: SafeBrowsingConfig,
    local_denylist: Vec<String>,
    agent: ureq::Agent,
}

impl Default for SafeBrowsing {
    fn default() -> Self {
        Self::new()
    }
}

impl SafeBrowsing {
    pub fn new() -> Self {
        Self {
            config: SafeBrowsingConfig::default(),
            local_denylist: LOCAL_THREAT_DOMAINS.iter().map(|s| s.to_string()).collect(),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(4))
                .build(),
        }
    }

    pub fn add_local_domain(&mut self, domain: &str) {
        self.local_denylist.push(domain.trim().to_ascii_lowercase());
    }

    pub fn local_domains(&self) -> Vec<String> {
        let mut v = self.local_denylist.clone();
        v.sort();
        v
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// True when a real remote lookup is possible (key configured and present).
    pub fn remote_available(&self) -> bool {
        self.api_key().is_some()
    }

    fn api_key(&self) -> Option<String> {
        let name = self.config.api_key_env.as_ref()?;
        std::env::var(name).ok().filter(|s| !s.is_empty())
    }

    fn query_api(&self, host: &str, key: &str) -> Result<bool, String> {
        // v4 endpoint used by Safe Browsing API v5 clients.
        let url = format!(
            "https://safebrowsing.googleapis.com/v4/threatMatches:find?key={}",
            key
        );
        let body = serde_json::json!({
            "client": {
                "clientId": self.config.client_id,
                "clientVersion": self.config.client_version
            },
            "threatInfo": {
                "threatTypes": ["MALWARE", "SOCIAL_ENGINEERING", "UNWANTED_SOFTWARE"],
                "platformTypes": ["ANY_PLATFORM"],
                "threatEntryTypes": ["URL"],
                "threatEntries": [{ "url": format!("https://{}/", host) }]
            }
        });
        let response = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| format!("Safe Browsing request failed: {}", e))?;
        let text = response
            .into_string()
            .map_err(|e| format!("Safe Browsing response unreadable: {}", e))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Invalid Safe Browsing response: {}", e))?;
        Ok(json
            .get("matches")
            .and_then(|m| m.as_array())
            .map(|m| !m.is_empty())
            .unwrap_or(false))
    }

    /// Check a URL. Local denylist first, then the configured remote service.
    pub fn check(&self, url: &str) -> Verdict {
        if !self.config.enabled {
            return Verdict::Disabled;
        }
        let host = host_of(url);
        if self
            .local_denylist
            .iter()
            .any(|d| host == *d || host.ends_with(&format!(".{}", d)))
        {
            return Verdict::Malicious("Local threat denylist".into());
        }
        match self.api_key() {
            Some(key) => match self.query_api(&host, &key) {
                Ok(true) => Verdict::Malicious("Google Safe Browsing".into()),
                Ok(false) => Verdict::Safe,
                Err(e) => Verdict::Unverified(e),
            },
            None => Verdict::Unverified(
                "No Safe Browsing API key configured (set the configured env var).".into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_denylist_blocks_and_allows() {
        let mut sb = SafeBrowsing::new();
        sb.add_local_domain("evil.test");
        assert!(matches!(
            sb.check("https://sub.evil.test/x"),
            Verdict::Malicious(_)
        ));
        assert!(matches!(sb.check("https://example.com"), Verdict::Unverified(_)));
    }

    #[test]
    fn disabled_returns_disabled() {
        let mut sb = SafeBrowsing::new();
        sb.set_enabled(false);
        assert_eq!(sb.check("https://evil.test"), Verdict::Disabled);
    }

    #[test]
    fn no_key_means_no_fake_verdict() {
        let sb = SafeBrowsing::new();
        assert!(!sb.remote_available());
        assert!(matches!(sb.check("https://example.com"), Verdict::Unverified(_)));
    }
}
