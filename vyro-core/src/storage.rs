//! Tiny JSON persistence helper shared by the stateful modules.
//!
//! Uses only the standard library plus serde/serde_json — no database dependency.
//! All writes are atomic (write to a temp file, then rename) so a crash cannot
//! leave a half-written settings/bookmarks file.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct JsonStore<T> {
    pub path: PathBuf,
    _marker: std::marker::PhantomData<T>,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Load the value from disk. Missing or unreadable files yield `None`.
    pub fn load(&self) -> Option<T> {
        let bytes = fs::read(&self.path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Atomically persist the value, returning an error on I/O failure.
    pub fn save(&self, value: &T) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp: PathBuf = PathBuf::from(format!("{}.tmp", self.path.display()));
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&tmp, bytes)?;
        // On Windows rename fails if the destination exists; remove it first.
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        fs::rename(&tmp, &self.path)
    }

    /// Load the value, or write and return `default` when absent.
    pub fn load_or(&self, default: T) -> T {
        self.load().unwrap_or_else(|| {
            let _ = self.save(&default);
            default
        })
    }
}

/// Convert a path-like string to a `PathBuf` (kept tiny for callers in commands.rs).
pub fn path_of(p: &str) -> PathBuf {
    Path::new(p).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        n: u32,
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("vyro-storage-test-{}", std::process::id()));
        let store = JsonStore::new(dir.join("v.json"));
        let value = Sample { n: 42 };
        store.save(&value).unwrap();
        assert_eq!(store.load(), Some(value));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_returns_none() {
        let store: JsonStore<Sample> =
            JsonStore::new(std::env::temp_dir().join("vyro-does-not-exist.json"));
        assert_eq!(store.load(), None);
    }
}
