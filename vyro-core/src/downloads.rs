//! Download manager state machine. The webview shell performs the byte transfer;
//! this module owns lifecycle, progress, and cancellation state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Pending,
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub target_path: String,
    pub received: u64,
    pub total: Option<u64>,
    pub state: DownloadState,
}

impl Download {
    pub fn progress(&self) -> f64 {
        match self.total {
            Some(t) if t > 0 => (self.received as f64 / t as f64).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
}

#[derive(Debug, Default)]
pub struct DownloadManager {
    items: Vec<Download>,
    next_id: u64,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, url: impl Into<String>, filename: impl Into<String>, target: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Download {
            id,
            url: url.into(),
            filename: filename.into(),
            target_path: target.into(),
            received: 0,
            total: None,
            state: DownloadState::Pending,
        });
        id
    }

    pub fn start(&mut self, id: u64) {
        self.set_state(id, DownloadState::InProgress);
    }

    pub fn set_total(&mut self, id: u64, total: u64) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.total = Some(total);
        }
    }

    pub fn progress(&mut self, id: u64, received: u64) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.received = received;
            d.state = DownloadState::InProgress;
        }
    }

    pub fn complete(&mut self, id: u64) {
        self.set_state(id, DownloadState::Completed);
    }

    pub fn fail(&mut self, id: u64, reason: impl Into<String>) {
        self.set_state(id, DownloadState::Failed(reason.into()));
    }

    pub fn cancel(&mut self, id: u64) {
        self.set_state(id, DownloadState::Cancelled);
    }

    fn set_state(&mut self, id: u64, state: DownloadState) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.state = state;
        }
    }

    pub fn get(&self, id: u64) -> Option<&Download> {
        self.items.iter().find(|d| d.id == id)
    }

    pub fn list(&self) -> &[Download] {
        &self.items
    }

    pub fn active_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| matches!(d.state, DownloadState::Pending | DownloadState::InProgress))
            .count()
    }

    pub fn clear_completed(&mut self) {
        self.items.retain(|d| {
            !matches!(
                d.state,
                DownloadState::Completed | DownloadState::Cancelled
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_progress() {
        let mut m = DownloadManager::new();
        let id = m.enqueue("https://a/x.zip", "x.zip", "/tmp/x.zip");
        m.start(id);
        m.set_total(id, 200);
        m.progress(id, 50);
        assert_eq!(m.get(id).unwrap().progress(), 0.25);
        m.complete(id);
        assert_eq!(m.get(id).unwrap().state, DownloadState::Completed);
        assert_eq!(m.active_count(), 0);
    }

    #[test]
    fn cancel_and_clear() {
        let mut m = DownloadManager::new();
        let id = m.enqueue("https://a/x", "x", "/tmp/x");
        m.cancel(id);
        m.clear_completed();
        assert!(m.list().is_empty());
    }
}
