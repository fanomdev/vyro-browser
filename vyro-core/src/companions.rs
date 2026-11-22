//! First-party companion extensions.
//!
//! A real alternative to Chrome extensions on webview engines: a companion is
//! a small JavaScript module injected into pages whose URL matches a pattern.
//! This is deliberately named and documented as a VYRO-specific format — it is
//! NOT Chrome Web Store compatibility.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    pub id: String,
    pub name: String,
    /// Glob patterns such as `*://*.example.com/*` or `*github*`.
    pub matches: Vec<String>,
    pub code: String,
    pub enabled: bool,
}

#[derive(Debug, Default)]
pub struct CompanionManager {
    items: Vec<Companion>,
}

impl CompanionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        matches: Vec<String>,
        code: impl Into<String>,
    ) -> Result<(), String> {
        let id = id.into();
        if self.items.iter().any(|c| c.id == id) {
            return Err(format!("Companion '{}' already exists", id));
        }
        self.items.push(Companion {
            id,
            name: name.into(),
            matches,
            code: code.into(),
            enabled: true,
        });
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|c| c.id != id);
        self.items.len() != before
    }

    pub fn toggle(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(c) = self.items.iter_mut().find(|c| c.id == id) {
            c.enabled = enabled;
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> &[Companion] {
        &self.items
    }

    fn matches(pattern: &str, url: &str) -> bool {
        // Small glob: `*` matches anything, `?` matches one char.
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = url.chars().collect();
        let (np, nt) = (p.len(), t.len());
        let mut dp = vec![vec![false; nt + 1]; np + 1];
        dp[0][0] = true;
        for i in 1..=np {
            for j in 0..=nt {
                dp[i][j] = match p[i - 1] {
                    '*' => dp[i - 1][j] || (j > 0 && dp[i][j - 1]),
                    '?' => j > 0 && dp[i - 1][j - 1],
                    c => j > 0 && t[j - 1] == c && dp[i - 1][j - 1],
                };
            }
        }
        dp[np][nt]
    }

    /// Code for companions that should run on this URL, each isolated so one
    /// broken companion cannot stop the others or the browser's own scripts.
    pub fn script_for(&self, url: &str) -> String {
        let mut out = String::new();
        for c in &self.items {
            if !c.enabled {
                continue;
            }
            if c.matches.iter().any(|m| Self::matches(m, url)) {
                out.push_str(&format!(
                    "try{{(function(){{\n{}\n}})();}}catch(e){{console.warn('VYRO companion {} failed',e);}}\n",
                    c.code, c.id
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_toggle_remove() {
        let mut m = CompanionManager::new();
        m.install("c1", "Test", vec!["*example.com*".into()], "console.log(1)")
            .unwrap();
        assert_eq!(m.list().len(), 1);
        assert!(m.install("c1", "Test", vec![], "").is_err());
        assert!(m.toggle("c1", false));
        assert!(m.script_for("https://example.com").is_empty());
        m.toggle("c1", true);
        assert!(!m.script_for("https://example.com").is_empty());
        assert!(m.script_for("https://other.org").is_empty());
        assert!(m.remove("c1"));
    }

    #[test]
    fn broken_companion_is_isolated() {
        let script = {
            let mut m = CompanionManager::new();
            m.install("bad", "Bad", vec!["*".into()], "throw new Error('x')")
                .unwrap();
            m.install("good", "Good", vec!["*".into()], "window.__ok=1")
                .unwrap();
            m.script_for("https://any.example")
        };
        assert!(script.contains("__ok=1"));
        assert!(script.contains("try{"));
    }
}
