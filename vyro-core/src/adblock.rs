//! Real network-level ad / tracker filter engine.
//!
//! Parses EasyList / EasyPrivacy-style filter rules (a useful, well-defined
//! subset), matches them against request URLs, honours exceptions and
//! per-site allowlisting, and keeps live protection statistics.
//!
//! The webview shell feeds eligible requests to [`Blocker::check`]; the engine
//! itself is pure and fully unit-tested. See `docs/LIMITATIONS.md` for exactly
//! which request types a given webview engine can expose for interception.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use url::Url;

/// Baseline built-in rules. EasyList/EasyPrivacy full lists are user-supplied
/// (downloaded at runtime) because of their size and licensing terms.
pub const BUILTIN_FILTERS: &str = r#"
! VYRO built-in baseline protection
||doubleclick.net^
||googlesyndication.com^
||googleadservices.com^
||google-analytics.com^
||googletagmanager.com^$script
||scorecardresearch.com^
||quantserve.com^
||amazon-adsystem.com^
||adservice.google.com^
||pagead2.googlesyndication.com^
||taboola.com^
||outbrain.com^
||criteo.com^
||facebook.net^$script
||connect.facebook.net^$script
||hotjar.com^
||mixpanel.com^
||segment.io^
||newrelic.com^$script
! keep local development usable
@@||localhost^
@@||127.0.0.1^
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceType {
    Script,
    Image,
    Stylesheet,
    XmlHttpRequest,
    Document,
    Font,
    Media,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockResult {
    pub blocked: bool,
    /// The raw filter rule that decided the request, when one matched.
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockerStats {
    pub blocked_total: u64,
    pub allowed_total: u64,
    pub blocked_by_domain: HashMap<String, u64>,
    /// Most recent blocked URLs (capped) for the live dashboard.
    pub recent: Vec<String>,
}

#[derive(Debug, Clone)]
enum Matcher {
    /// `||domain^` — matches the domain and all its subdomains.
    DomainAnchor(String),
    /// Glob with `*` (any sequence) and `^` (URL separator) wildcards.
    Wildcard(String),
    /// `|prefix` — anchored at the start of the address.
    ExactPrefix(String),
}

#[derive(Debug, Clone)]
pub struct Filter {
    raw: String,
    allow: bool,
    matcher: Matcher,
    third_party_only: bool,
    first_party_only: bool,
    types: Option<Vec<ResourceType>>,
    /// Source list (e.g. "builtin", "easylist") — kept for per-list reporting.
    #[allow(dead_code)]
    category: String,
}

impl Filter {
    fn matches(
        &self,
        url: &str,
        host: &str,
        rtype: ResourceType,
        third_party: bool,
    ) -> bool {
        if self.third_party_only && !third_party {
            return false;
        }
        if self.first_party_only && third_party {
            return false;
        }
        if let Some(types) = &self.types {
            if !types.contains(&rtype) {
                return false;
            }
        }
        match &self.matcher {
            Matcher::DomainAnchor(domain) => {
                host == domain || host.ends_with(&format!(".{}", domain))
            }
            Matcher::Wildcard(pattern) => abp_match(pattern, url),
            Matcher::ExactPrefix(prefix) => url.starts_with(prefix),
        }
    }
}

/// ABP-style wildcard match: `*` = any sequence, `^` = a single separator
/// (anything that is not alphanumeric, `.`, `-`, `_`, `%`) or end of string.
/// Implemented with dynamic programming — O(pattern_len * text_len), no regex dep.
fn abp_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (np, nt) = (p.len(), t.len());
    let mut dp = vec![vec![false; nt + 1]; np + 1];
    dp[0][0] = true;

    for i in 1..=np {
        for j in 0..=nt {
            dp[i][j] = match p[i - 1] {
                '*' => dp[i - 1][j] || (j > 0 && dp[i][j - 1]),
                '^' => {
                    if j == nt {
                        dp[i - 1][j]
                    } else {
                        let c = t[j];
                        let is_sep = !(c.is_alphanumeric()
                            || c == '.'
                            || c == '-'
                            || c == '_'
                            || c == '%');
                        dp[i - 1][j] && is_sep
                    }
                }
                c => j > 0 && t[j - 1] == c && dp[i - 1][j - 1],
            };
        }
    }
    dp[np][nt]
}

/// Extract the host from a URL without panicking on odd input.
fn host_of(url: &str) -> String {
    if let Ok(u) = Url::parse(url) {
        if let Some(h) = u.host_str() {
            return h.to_ascii_lowercase();
        }
    }
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', ':', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn parse_resource_type(token: &str) -> Option<ResourceType> {
    match token {
        "script" | "scriptlet" => Some(ResourceType::Script),
        "image" | "img" => Some(ResourceType::Image),
        "stylesheet" | "css" => Some(ResourceType::Stylesheet),
        "xmlhttprequest" | "xhr" | "fetch" => Some(ResourceType::XmlHttpRequest),
        "document" => Some(ResourceType::Document),
        "font" => Some(ResourceType::Font),
        "media" => Some(ResourceType::Media),
        _ => None,
    }
}

/// Parse one filter-list line into a [`Filter`]. Returns `None` for comments,
/// headers, and malformed/unsupported rules.
pub fn parse_line(line: &str, category: &str) -> Option<Filter> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
        return None;
    }

    let mut body = line;
    let allow = if let Some(rest) = body.strip_prefix("@@") {
        body = rest;
        true
    } else {
        false
    };

    let mut third_party_only = false;
    let mut first_party_only = false;
    let mut types: Option<Vec<ResourceType>> = None;
    if let Some(idx) = body.rfind('$') {
        let options = &body[idx + 1..];
        body = &body[..idx];
        let mut collected = Vec::new();
        for opt in options.split(',') {
            let opt = opt.trim();
            match opt {
                "third-party" => third_party_only = true,
                "~third-party" => first_party_only = true,
                "" => {}
                other => {
                    if let Some(rt) = parse_resource_type(other.trim_start_matches('~')) {
                        collected.push(rt);
                    }
                    // Unknown options (e.g. `domain=`) are ignored rather than
                    // misapplied — a wrong block is worse than a missed one.
                }
            }
        }
        if !collected.is_empty() {
            types = Some(collected);
        }
    }

    if body.is_empty() {
        return None;
    }

    let matcher = if let Some(rest) = body.strip_prefix("||") {
        Matcher::DomainAnchor(rest.trim_end_matches('^').to_ascii_lowercase())
    } else if let Some(rest) = body.strip_prefix('|') {
        Matcher::ExactPrefix(rest.to_string())
    } else {
        Matcher::Wildcard(format!("*{}*", body.trim_matches('*')))
    };

    Some(Filter {
        raw: line.to_string(),
        allow,
        matcher,
        third_party_only,
        first_party_only,
        types,
        category: category.to_string(),
    })
}

#[derive(Debug)]
pub struct Blocker {
    filters: Vec<Filter>,
    enabled: bool,
    allowlisted_sites: HashSet<String>,
    pub stats: BlockerStats,
    recent_cap: usize,
}

impl Default for Blocker {
    fn default() -> Self {
        Self::new()
    }
}

impl Blocker {
    pub fn new() -> Self {
        let mut b = Self {
            filters: Vec::new(),
            enabled: true,
            allowlisted_sites: HashSet::new(),
            stats: BlockerStats::default(),
            recent_cap: 50,
        };
        b.add_filters(BUILTIN_FILTERS, "builtin");
        b
    }

    /// Load a filter list in EasyList/EasyPrivacy text format.
    /// Returns the number of rules added.
    pub fn add_filters(&mut self, text: &str, category: &str) -> usize {
        let mut added = 0;
        for line in text.lines() {
            if let Some(f) = parse_line(line, category) {
                self.filters.push(f);
                added += 1;
            }
        }
        added
    }

    pub fn filter_count(&self) -> usize {
        self.filters.len()
    }

    /// Domains from `||domain^` rules, used to build the webview's in-page
    /// interception bridge (the only hook system webviews expose).
    pub fn blocked_domains(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .filters
            .iter()
            .filter_map(|f| match &f.matcher {
                Matcher::DomainAnchor(d) if !f.allow => Some(d.clone()),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Globally allow a site (by host or domain suffix) so it is never filtered.
    pub fn allow_site(&mut self, site: &str) {
        self.allowlisted_sites
            .insert(site.trim().to_ascii_lowercase());
    }

    pub fn unallow_site(&mut self, site: &str) {
        self.allowlisted_sites.remove(&site.trim().to_ascii_lowercase());
    }

    pub fn is_site_allowed(&self, site: &str) -> bool {
        let site = site.trim().to_ascii_lowercase();
        self.allowlisted_sites
            .iter()
            .any(|a| site == *a || site.ends_with(&format!(".{}", a)))
    }

    pub fn allowlisted_sites(&self) -> Vec<String> {
        let mut v: Vec<String> = self.allowlisted_sites.iter().cloned().collect();
        v.sort();
        v
    }

    /// Decide whether a request should be blocked.
    ///
    /// `source_domain` is the document making the request (for allowlisting and
    /// first/third-party classification), `rtype` is the resource category, and
    /// `third_party` is the engine's own classification when available.
    pub fn check(
        &mut self,
        url: &str,
        source_domain: &str,
        rtype: ResourceType,
        third_party: bool,
    ) -> BlockResult {
        if !self.enabled || self.is_site_allowed(source_domain) {
            return BlockResult {
                blocked: false,
                filter: None,
            };
        }
        let host = host_of(url);
        let is_third_party = third_party || (!source_domain.is_empty() && host != source_domain);

        // Exceptions win — scan allow filters first.
        for f in &self.filters {
            if f.allow && f.matches(url, &host, rtype, is_third_party) {
                self.stats.allowed_total += 1;
                return BlockResult {
                    blocked: false,
                    filter: Some(f.raw.clone()),
                };
            }
        }
        for f in &self.filters {
            if !f.allow && f.matches(url, &host, rtype, is_third_party) {
                self.stats.blocked_total += 1;
                *self.stats.blocked_by_domain.entry(host.clone()).or_insert(0) += 1;
                if self.stats.recent.len() == self.recent_cap {
                    self.stats.recent.remove(0);
                }
                self.stats.recent.push(url.to_string());
                return BlockResult {
                    blocked: true,
                    filter: Some(f.raw.clone()),
                };
            }
        }
        BlockResult {
            blocked: false,
            filter: None,
        }
    }

    pub fn stats_snapshot(&self) -> BlockerStats {
        self.stats.clone()
    }

    pub fn reset_stats(&mut self) {
        self.stats = BlockerStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_headers_are_ignored() {
        assert!(parse_line("! a comment", "test").is_none());
        assert!(parse_line("[Adblock Plus 2.0]", "test").is_none());
        assert!(parse_line("", "test").is_none());
        assert!(parse_line("||example.com^", "test").is_some());
    }

    #[test]
    fn domain_anchor_blocks_domain_and_subdomains() {
        let mut b = Blocker::new();
        let r = b.check(
            "https://ad.doubleclick.net/ads/x.js",
            "news.example.com",
            ResourceType::Script,
            true,
        );
        assert!(r.blocked);
        assert_eq!(r.filter.as_deref(), Some("||doubleclick.net^"));
    }

    #[test]
    fn domain_anchor_does_not_match_lookalike() {
        let mut b = Blocker::new();
        let r = b.check(
            "https://notdoubleclick.net/x.js",
            "news.example.com",
            ResourceType::Script,
            true,
        );
        assert!(!r.blocked);
    }

    #[test]
    fn exception_rule_unblocks() {
        let mut b = Blocker::new();
        b.add_filters("@@||googletagmanager.com^", "test");
        let r = b.check(
            "https://www.googletagmanager.com/gtm.js",
            "example.com",
            ResourceType::Script,
            true,
        );
        assert!(!r.blocked);
        assert!(r.filter.unwrap().starts_with("@@"));
    }

    #[test]
    fn resource_type_option_is_respected() {
        let mut b = Blocker::new();
        // builtin: ||googletagmanager.com^$script
        assert!(b
            .check(
                "https://www.googletagmanager.com/gtm.js",
                "example.com",
                ResourceType::Script,
                true,
            )
            .blocked);
        assert!(!b
            .check(
                "https://www.googletagmanager.com/img.png",
                "example.com",
                ResourceType::Image,
                true,
            )
            .blocked);
    }

    #[test]
    fn third_party_option_is_respected() {
        let mut b = Blocker::new();
        b.add_filters("||tracker.example$third-party", "test");
        assert!(b
            .check(
                "https://cdn.tracker.example/t.js",
                "news.example.com",
                ResourceType::Script,
                true,
            )
            .blocked);
        assert!(!b
            .check(
                "https://news.tracker.example/t.js",
                "news.tracker.example",
                ResourceType::Script,
                false,
            )
            .blocked);
    }

    #[test]
    fn site_allowlist_disables_blocking() {
        let mut b = Blocker::new();
        b.allow_site("example.com");
        assert!(b.is_site_allowed("sub.example.com"));
        assert!(!b
            .check(
                "https://ad.doubleclick.net/x",
                "sub.example.com",
                ResourceType::Image,
                true,
            )
            .blocked);
        b.unallow_site("example.com");
        assert!(b
            .check(
                "https://ad.doubleclick.net/x",
                "sub.example.com",
                ResourceType::Image,
                true,
            )
            .blocked);
    }

    #[test]
    fn global_toggle_disables_blocking() {
        let mut b = Blocker::new();
        b.set_enabled(false);
        assert!(!b
            .check(
                "https://ad.doubleclick.net/x",
                "example.com",
                ResourceType::Script,
                true,
            )
            .blocked);
    }

    #[test]
    fn stats_track_blocks_per_domain() {
        let mut b = Blocker::new();
        for _ in 0..3 {
            b.check(
                "https://ad.doubleclick.net/x",
                "example.com",
                ResourceType::Image,
                true,
            );
        }
        let s = b.stats_snapshot();
        assert_eq!(s.blocked_total, 3);
        assert_eq!(s.blocked_by_domain.get("ad.doubleclick.net"), Some(&3));
        assert_eq!(s.recent.len(), 3);
    }

    #[test]
    fn wildcard_matcher_handles_stars_and_separators() {
        assert!(abp_match("*/ads/*", "https://site.com/ads/banner.js"));
        assert!(!abp_match("*/ads/*", "https://site.com/articles/x"));
        assert!(abp_match("*tracker*^", "https://x.com/tracker?a=1"));
    }
}
