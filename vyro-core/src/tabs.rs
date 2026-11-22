//! Tab and tab-group state, including per-tab navigation history.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub private: bool,
    pub group: Option<String>,
    back_stack: Vec<String>,
    forward_stack: Vec<String>,
}

impl Tab {
    fn new(id: u64, url: String, private: bool) -> Self {
        Self {
            id,
            title: url.clone(),
            url,
            loading: false,
            private,
            group: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
        }
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }
}

#[derive(Debug)]
pub struct TabManager {
    tabs: Vec<Tab>,
    active: Option<u64>,
    next_id: u64,
    max_tabs: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
            max_tabs: 100,
        }
    }

    pub fn open(&mut self, url: impl Into<String>, private: bool) -> u64 {
        // Every "new tab" request makes a real new tab. Reusing a blank one made
        // the + button appear broken after the first click.
        if self.tabs.len() >= self.max_tabs {
            self.close(self.tabs.last().unwrap().id);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, url.into(), private));
        self.active = Some(id);
        id
    }

    pub fn close(&mut self, id: u64) -> bool {
        let before = self.tabs.len();
        self.tabs.retain(|t| t.id != id);
        if self.tabs.len() == before {
            return false;
        }
        if self.active == Some(id) {
            self.active = self.tabs.last().map(|t| t.id);
        }
        true
    }

    pub fn set_active(&mut self, id: u64) -> bool {
        if self.tabs.iter().any(|t| t.id == id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tabs.iter().find(|t| t.id == id))
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active
    }

    pub fn get(&self, id: u64) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn list(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn count(&self) -> usize {
        self.tabs.len()
    }

    /// Record a completed navigation, pushing the previous URL onto the back stack.
    pub fn navigate(&mut self, id: u64, url: impl Into<String>) -> bool {
        let url = url.into();
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            if !tab.url.is_empty() && tab.url != url {
                tab.back_stack.push(tab.url.clone());
                tab.forward_stack.clear();
            }
            tab.url = url;
            tab.loading = false;
            true
        } else {
            false
        }
    }

    pub fn set_title(&mut self, id: u64, title: impl Into<String>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.title = title.into();
        }
    }

    pub fn set_loading(&mut self, id: u64, loading: bool) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.loading = loading;
        }
    }

    pub fn set_group(&mut self, id: u64, group: Option<String>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.group = group;
        }
    }

    pub fn groups(&self) -> Vec<(String, Vec<u64>)> {
        let mut out: Vec<(String, Vec<u64>)> = Vec::new();
        for tab in &self.tabs {
            if let Some(group) = &tab.group {
                if let Some((_, ids)) = out.iter_mut().find(|(g, _)| g == group) {
                    ids.push(tab.id);
                } else {
                    out.push((group.clone(), vec![tab.id]));
                }
            }
        }
        out
    }

    pub fn back(&mut self, id: u64) -> Option<String> {
        let tab = self.tabs.iter_mut().find(|t| t.id == id)?;
        let prev = tab.back_stack.pop()?;
        tab.forward_stack.push(tab.url.clone());
        tab.url = prev.clone();
        Some(prev)
    }

    pub fn forward(&mut self, id: u64) -> Option<String> {
        let tab = self.tabs.iter_mut().find(|t| t.id == id)?;
        let next = tab.forward_stack.pop()?;
        tab.back_stack.push(tab.url.clone());
        tab.url = next.clone();
        Some(next)
    }

    /// Move a tab to a new position so users can reorder by dragging.
    pub fn move_tab(&mut self, id: u64, to: usize) -> bool {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        let target = to.min(self.tabs.len() - 1);
        let tab = self.tabs.remove(from);
        self.tabs.insert(target, tab);
        true
    }

    pub fn close_others(&mut self, keep: u64) {
        self.tabs.retain(|t| t.id == keep);
        self.active = Some(keep);
    }

    /// Serialize only what is needed to restore a session.
    pub fn session_snapshot(&self) -> Vec<(String, String, bool)> {
        self.tabs
            .iter()
            .map(|t| (t.url.clone(), t.title.clone(), t.private))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_navigate_back_forward() {
        let mut m = TabManager::new();
        let id = m.open("about:blank", false);
        m.navigate(id, "https://a.example");
        m.navigate(id, "https://b.example");
        assert_eq!(m.get(id).unwrap().url, "https://b.example");
        assert_eq!(m.back(id).as_deref(), Some("https://a.example"));
        assert_eq!(m.forward(id).as_deref(), Some("https://b.example"));
    }

    #[test]
    fn opening_twice_creates_two_tabs() {
        let mut m = TabManager::new();
        let a = m.open("", false);
        let b = m.open("", false);
        assert_ne!(a, b);
        assert_eq!(m.count(), 2);
        assert_eq!(m.active_id(), Some(b));
    }

    #[test]
    fn closing_active_moves_to_last_tab() {
        let mut m = TabManager::new();
        let a = m.open("https://a.example", false);
        let b = m.open("https://b.example", false);
        assert_eq!(m.active_id(), Some(b));
        assert!(m.close(b));
        assert_eq!(m.active_id(), Some(a));
        assert!(!m.close(999));
    }

    #[test]
    fn tabs_can_be_reordered() {
        let mut m = TabManager::new();
        let a = m.open("a", false);
        let b = m.open("b", false);
        let c = m.open("c", false);
        assert!(m.move_tab(c, 0));
        let urls: Vec<&str> = m.list().iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, vec!["c", "a", "b"]);
        m.move_tab(a, 99);
        assert_eq!(m.list()[2].id, a);
        assert!(!m.move_tab(999, 0));
    }

    #[test]
    fn groups_are_tracked() {
        let mut m = TabManager::new();
        let a = m.open("https://a.example", false);
        let b = m.open("https://b.example", false);
        m.set_group(a, Some("Work".into()));
        m.set_group(b, Some("Work".into()));
        let groups = m.groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
    }
}
