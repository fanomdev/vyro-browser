//! Privacy controls: site permissions, cookie policy, HTTPS upgrades, and
//! browsing-data deletion categories.

use crate::engine::upgrade_to_https;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Camera,
    Microphone,
    Location,
    Notifications,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Block,
    Ask,
}

impl Default for Decision {
    fn default() -> Self {
        Decision::Ask
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CookiePolicy {
    AllowAll,
    BlockThirdParty,
    BlockAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub cookie_policy: CookiePolicy,
    pub https_upgrade: bool,
    pub tracking_protection: bool,
    pub fingerprinting_protection: bool,
    pub safe_browsing: bool,
    pub do_not_track: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            cookie_policy: CookiePolicy::BlockThirdParty,
            https_upgrade: true,
            tracking_protection: true,
            fingerprinting_protection: false,
            safe_browsing: true,
            do_not_track: true,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DataToClear {
    pub history: bool,
    pub cookies: bool,
    pub cache: bool,
    pub local_storage: bool,
    pub downloads: bool,
    pub passwords: bool,
}

#[derive(Debug, Default)]
pub struct PermissionStore {
    decisions: HashMap<String, Decision>,
}

fn key(origin: &str, permission: Permission) -> String {
    format!("{}|{:?}", origin.trim().to_ascii_lowercase(), permission)
}

impl PermissionStore {
    pub fn decision(&self, origin: &str, permission: Permission) -> Decision {
        self.decisions
            .get(&key(origin, permission))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&mut self, origin: &str, permission: Permission, decision: Decision) {
        self.decisions.insert(key(origin, permission), decision);
    }

    pub fn reset(&mut self, origin: &str, permission: Permission) {
        self.decisions.remove(&key(origin, permission));
    }

    pub fn reset_origin(&mut self, origin: &str) {
        let prefix = format!("{}|", origin.trim().to_ascii_lowercase());
        self.decisions.retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn clear(&mut self) {
        self.decisions.clear();
    }

    pub fn all(&self) -> Vec<(String, Decision)> {
        self.decisions.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

#[derive(Debug)]
pub struct PrivacyController {
    pub config: PrivacyConfig,
    pub permissions: PermissionStore,
}

impl Default for PrivacyController {
    fn default() -> Self {
        Self {
            config: PrivacyConfig::default(),
            permissions: PermissionStore::default(),
        }
    }
}

impl PrivacyController {
    /// Apply the configured HTTPS-upgrade policy to a navigated URL.
    /// Returns the upgraded URL when the policy is on and the URL is http.
    pub fn apply_policies(&self, url: &str) -> String {
        if self.config.https_upgrade {
            upgrade_to_https(url).unwrap_or_else(|| url.to_string())
        } else {
            url.to_string()
        }
    }

    /// Whether a third-party cookie should be accepted for this request.
    pub fn allow_cookie(&self, third_party: bool) -> bool {
        match self.config.cookie_policy {
            CookiePolicy::AllowAll => true,
            CookiePolicy::BlockThirdParty => !third_party,
            CookiePolicy::BlockAll => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_defaults_to_ask_and_persists() {
        let mut p = PermissionStore::default();
        assert_eq!(p.decision("https://a.example", Permission::Camera), Decision::Ask);
        p.set("https://a.example", Permission::Camera, Decision::Allow);
        assert_eq!(p.decision("https://a.example", Permission::Camera), Decision::Allow);
        p.reset("https://a.example", Permission::Camera);
        assert_eq!(p.decision("https://a.example", Permission::Camera), Decision::Ask);
    }

    #[test]
    fn reset_origin_clears_all_its_permissions() {
        let mut p = PermissionStore::default();
        p.set("https://a.example", Permission::Camera, Decision::Allow);
        p.set("https://a.example", Permission::Location, Decision::Allow);
        p.reset_origin("https://a.example");
        assert!(p.all().is_empty());
    }

    #[test]
    fn https_policy_and_cookie_policy() {
        let c = PrivacyController::default();
        assert_eq!(
            c.apply_policies("http://example.com/x"),
            "https://example.com/x"
        );
        assert!(!c.allow_cookie(true));
        assert!(c.allow_cookie(false));
    }
}
