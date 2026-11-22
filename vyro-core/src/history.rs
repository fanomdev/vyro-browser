//! Browsing history with visit aggregation, search, and selective deletion.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: u64,
    pub visit_count: u32,
}

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<HistoryEntry>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a visit. Revisiting an existing URL bumps its count and timestamp.
    pub fn visit(&mut self, url: impl Into<String>, title: impl Into<String>) {
        let url = url.into();
        let title = title.into();
        if let Some(e) = self.entries.iter_mut().find(|e| e.url == url) {
            e.visit_count += 1;
            e.visited_at = now();
            if !title.is_empty() {
                e.title = title;
            }
        } else {
            self.entries.push(HistoryEntry {
                url,
                title,
                visited_at: now(),
                visit_count: 1,
            });
        }
    }

    pub fn list(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Newest first; an empty query returns all.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&HistoryEntry> {
        let q = query.to_ascii_lowercase();
        let mut matches: Vec<&HistoryEntry> = self
            .entries
            .iter()
            .filter(|e| {
                q.is_empty()
                    || e.title.to_ascii_lowercase().contains(&q)
                    || e.url.to_ascii_lowercase().contains(&q)
            })
            .collect();
        matches.sort_by(|a, b| b.visited_at.cmp(&a.visited_at));
        matches.truncate(limit);
        matches
    }

    pub fn most_visited(&self, limit: usize) -> Vec<&HistoryEntry> {
        let mut v: Vec<&HistoryEntry> = self.entries.iter().collect();
        v.sort_by(|a, b| b.visit_count.cmp(&a.visit_count));
        v.truncate(limit);
        v
    }

    pub fn remove_url(&mut self, url: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.url != url);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Restore entries from a previous session without duplicating URLs.
    pub fn import(&mut self, entries: Vec<HistoryEntry>) {
        for e in entries {
            if !self.entries.iter().any(|x| x.url == e.url) {
                self.entries.push(e);
            }
        }
    }

    /// Remove everything visited strictly before `cutoff` (unix seconds).
    pub fn clear_before(&mut self, cutoff: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.visited_at >= cutoff);
        before - self.entries.len()
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_are_aggregated() {
        let mut h = History::new();
        h.visit("https://a.example", "A");
        h.visit("https://a.example", "A title");
        assert_eq!(h.count(), 1);
        assert_eq!(h.entries[0].visit_count, 2);
        assert_eq!(h.entries[0].title, "A title");
    }

    #[test]
    fn search_and_most_visited() {
        let mut h = History::new();
        h.visit("https://rust-lang.org", "Rust");
        h.visit("https://a.example", "A");
        h.visit("https://a.example", "A");
        assert_eq!(h.search("rust", 10).len(), 1);
        assert_eq!(h.most_visited(1)[0].url, "https://a.example");
    }

    #[test]
    fn selective_clear() {
        let mut h = History::new();
        h.visit("https://a.example", "A");
        assert!(h.remove_url("https://a.example"));
        h.visit("https://b.example", "B");
        h.clear();
        assert_eq!(h.count(), 0);
    }
}
