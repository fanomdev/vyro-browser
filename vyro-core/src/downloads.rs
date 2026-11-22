//! Download manager state machine and persistence. The shell's Rust transfer
//! engine performs the byte transfer; this module owns lifecycle, progress and
//! resumable state so interrupted downloads can continue after a restart.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Pending,
    InProgress,
    Paused,
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

    pub fn pause(&mut self, id: u64) {
        self.set_state(id, DownloadState::Paused);
    }

    pub fn resume(&mut self, id: u64) {
        self.set_state(id, DownloadState::InProgress);
    }

    /// Persist download state so an interrupted transfer can be resumed.
    pub fn export(&self) -> String {
        serde_json::to_string_pretty(&self.items).unwrap_or_default()
    }

    pub fn import(&mut self, json: &str) {
        if let Ok(items) = serde_json::from_str::<Vec<Download>>(json) {
            self.next_id = items.iter().map(|d| d.id + 1).max().unwrap_or(1);
            self.items = items;
        }
    }

    /// Downloads that were active when the app last exited.
    pub fn unfinished_ids(&self) -> Vec<u64> {
        self.items
            .iter()
            .filter(|d| matches!(d.state, DownloadState::Pending | DownloadState::InProgress))
            .map(|d| d.id)
            .collect()
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
    fn pause_resume_and_persist() {
        let mut m = DownloadManager::new();
        let id = m.enqueue("https://a/f.zip", "f.zip", "/tmp/f.zip");
        m.start(id);
        m.progress(id, 500);
        m.pause(id);
        assert_eq!(m.get(id).unwrap().state, DownloadState::Paused);
        let json = m.export();
        let mut restored = DownloadManager::new();
        restored.import(&json);
        assert_eq!(restored.get(id).unwrap().received, 500);
        assert_eq!(restored.get(id).unwrap().state, DownloadState::Paused);
        restored.resume(id);
        assert_eq!(restored.get(id).unwrap().state, DownloadState::InProgress);
        assert_eq!(restored.unfinished_ids(), vec![id]);
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
