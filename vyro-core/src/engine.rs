//! Navigation intent resolution — the browser's "address bar brain".
//!
//! Turns raw user input into either a URL to load or a search query to run,
//! using the selected search engine. Kept free of webview concerns for testing.

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchEngine {
    DuckDuckGo,
    Brave,
    Google,
    Bing,
    Startpage,
    /// Routes tab traffic through a local Tor SOCKS proxy; clearnet search via DDG.
    Tor,
}

impl Default for SearchEngine {
    fn default() -> Self {
        SearchEngine::DuckDuckGo
    }
}

impl SearchEngine {
    pub fn label(self) -> &'static str {
        match self {
            SearchEngine::DuckDuckGo => "DuckDuckGo",
            SearchEngine::Brave => "Brave",
            SearchEngine::Google => "Google",
            SearchEngine::Bing => "Bing",
            SearchEngine::Startpage => "Startpage",
            SearchEngine::Tor => "Tor (SOCKS proxy)",
        }
    }

    /// Search URL template with `{}` as the query placeholder.
    pub fn template(self) -> &'static str {
        match self {
            SearchEngine::DuckDuckGo => "https://duckduckgo.com/?q={}",
            SearchEngine::Brave => "https://search.brave.com/search?q={}",
            SearchEngine::Google => "https://www.google.com/search?q={}",
            SearchEngine::Bing => "https://www.bing.com/search?q={}",
            SearchEngine::Startpage => "https://www.startpage.com/sp/search?query={}",
            SearchEngine::Tor => "https://duckduckgo.com/?q={}",
        }
    }

    pub fn search_url(self, query: &str) -> String {
        self.template().replace("{}", &percent_encode(query))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationIntent {
    Url(String),
    Search(String),
}

/// Minimal percent-encoding for query strings (unreserved chars pass through).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Heuristic: does the input look like an explicit URL or host?
fn looks_like_url(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return true;
    }
    if lower.starts_with("about:")
        || lower.starts_with("file:")
        || lower.starts_with("data:")
        || lower.starts_with("vyro:")
    {
        return true;
    }
    // localhost / IP / domain with a dot and no spaces
    if !input.contains(char::is_whitespace) {
        if lower == "localhost" || lower.starts_with("localhost:") {
            return true;
        }
        if lower.starts_with("http") {
            return true;
        }
        let hostpart = lower.split('/').next().unwrap_or("");
        if hostpart.contains('.') && !hostpart.ends_with('.') {
            let tldish = hostpart.rsplit('.').next().unwrap_or("");
            // avoid treating "3.14" or "file.txt" as a host: require alpha TLD
            if tldish.chars().all(|c| c.is_ascii_alphabetic()) && tldish.len() >= 2 {
                return true;
            }
        }
    }
    false
}

/// Resolve address-bar input into a navigation intent.
///
/// A bare domain becomes `https://…`; a multi-word string becomes a search.
pub fn resolve(input: &str, engine: SearchEngine) -> NavigationIntent {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return NavigationIntent::Search(String::new());
    }
    if looks_like_url(trimmed) {
        if trimmed.to_ascii_lowercase().starts_with("http://")
            || trimmed.to_ascii_lowercase().starts_with("https://")
            || trimmed.contains(':')
        {
            return NavigationIntent::Url(trimmed.to_string());
        }
        return NavigationIntent::Url(format!("https://{}", trimmed));
    }
    NavigationIntent::Search(engine.search_url(trimmed))
}

/// Upgrade an `http://` URL to `https://` when it is safe to try.
/// Real HSTS preload is delegated to the engine; this is the best-effort layer.
pub fn upgrade_to_https(url: &str) -> Option<String> {
    if url.to_ascii_lowercase().starts_with("http://") {
        let parsed = Url::parse(url).ok()?;
        let mut upgraded = parsed.clone();
        upgraded.set_scheme("https").ok()?;
        let mut s = upgraded.to_string();
        if let Some(port) = parsed.port() {
            if port == 80 {
                s = s.replace(":80/", "/");
            }
        }
        Some(s)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bare_domain_to_https() {
        assert_eq!(
            resolve("example.com", SearchEngine::DuckDuckGo),
            NavigationIntent::Url("https://example.com".into())
        );
    }

    #[test]
    fn resolves_query_to_search() {
        let intent = resolve("rust browser engine", SearchEngine::Brave);
        match intent {
            NavigationIntent::Search(u) => {
                assert!(u.starts_with("https://search.brave.com/search?q="));
                assert!(u.contains("rust+browser+engine"));
            }
            _ => panic!("expected search"),
        }
    }

    #[test]
    fn keeps_explicit_scheme() {
        assert_eq!(
            resolve("http://localhost:8080/path", SearchEngine::Google),
            NavigationIntent::Url("http://localhost:8080/path".into())
        );
    }

    #[test]
    fn upgrades_http_to_https() {
        assert_eq!(
            upgrade_to_https("http://example.com/x"),
            Some("https://example.com/x".into())
        );
        assert_eq!(upgrade_to_https("https://example.com"), None);
    }

    #[test]
    fn treats_ip_like_number_as_search() {
        assert!(matches!(
            resolve("3.14", SearchEngine::Google),
            NavigationIntent::Search(_)
        ));
    }
}
