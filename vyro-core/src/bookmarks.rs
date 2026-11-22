//! Bookmarks with folders, search, and import/export (JSON + Netscape HTML).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub folder: String,
    pub favicon: Option<String>,
}

#[derive(Debug, Default)]
pub struct BookmarkStore {
    items: Vec<Bookmark>,
    next_id: u64,
}

impl BookmarkStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, title: impl Into<String>, url: impl Into<String>, folder: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Bookmark {
            id,
            title: title.into(),
            url: url.into(),
            folder: folder.into(),
            favicon: None,
        });
        id
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|b| b.id != id);
        self.items.len() != before
    }

    pub fn update(&mut self, id: u64, title: String, url: String, folder: String) -> bool {
        if let Some(b) = self.items.iter_mut().find(|b| b.id == id) {
            b.title = title;
            b.url = url;
            b.folder = folder;
            true
        } else {
            false
        }
    }

    pub fn get(&self, id: u64) -> Option<&Bookmark> {
        self.items.iter().find(|b| b.id == id)
    }

    pub fn list(&self) -> &[Bookmark] {
        &self.items
    }

    pub fn in_folder(&self, folder: &str) -> Vec<&Bookmark> {
        self.items.iter().filter(|b| b.folder == folder).collect()
    }

    pub fn folders(&self) -> Vec<String> {
        let mut f: Vec<String> = self.items.iter().map(|b| b.folder.clone()).collect();
        f.sort();
        f.dedup();
        f
    }

    pub fn search(&self, query: &str) -> Vec<&Bookmark> {
        let q = query.to_ascii_lowercase();
        self.items
            .iter()
            .filter(|b| {
                b.title.to_ascii_lowercase().contains(&q)
                    || b.url.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    /// Import from a Netscape bookmark HTML file (the format Chrome/Firefox export).
    pub fn import_netscape_html(&mut self, html: &str) -> usize {
        let mut count = 0;
        let mut rest = html;
        while let Some(pos) = rest.find("<A ").or_else(|| rest.find("<a ")) {
            rest = &rest[pos..];
            let end = match rest.find('>') {
                Some(e) => e,
                None => break,
            };
            let tag = &rest[..end];
            let href = attr(tag, "href").or_else(|| attr(tag, "HREF")).unwrap_or_default();
            let title = strip_tags_between(tag, end, rest);
            if !href.is_empty() {
                let title = if title.trim().is_empty() {
                    href.clone()
                } else {
                    title
                };
                self.add(title, href, "Imported");
                count += 1;
            }
            rest = &rest[end + 1..];
        }
        count
    }

    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.items).unwrap_or_default()
    }

    pub fn import_json(&mut self, json: &str) -> Result<usize, String> {
        let imported: Vec<Bookmark> =
            serde_json::from_str(json).map_err(|e| format!("Invalid bookmark file: {}", e))?;
        let n = imported.len();
        for mut b in imported {
            b.id = self.next_id;
            self.next_id += 1;
            self.items.push(b);
        }
        Ok(n)
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{}=", name);
    let i = tag.find(&needle)? + needle.len();
    let rest = &tag[i..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = rest[1..].find(quote)? + 1;
    Some(unescape_html(&rest[1..end]))
}

fn strip_tags_between(tag: &str, _end: usize, rest: &str) -> String {
    // Text between `>tag<` and the next `</A>`.
    let after = &rest[tag.len() + 1..];
    let text = after.split("</").next().unwrap_or("");
    unescape_html(text.trim())
}

fn unescape_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_search_and_folders() {
        let mut s = BookmarkStore::new();
        s.add("Rust", "https://rust-lang.org", "Dev");
        s.add("Duck", "https://duckduckgo.com", "Search");
        assert_eq!(s.search("rust").len(), 1);
        assert_eq!(s.folders(), vec!["Dev", "Search"]);
        assert_eq!(s.in_folder("Dev").len(), 1);
    }

    #[test]
    fn update_and_remove() {
        let mut s = BookmarkStore::new();
        let id = s.add("A", "https://a.example", "F");
        assert!(s.update(id, "B".into(), "https://b.example".into(), "G".into()));
        assert_eq!(s.get(id).unwrap().title, "B");
        assert!(s.remove(id));
        assert!(s.list().is_empty());
    }

    #[test]
    fn netscape_import_parses_links() {
        let mut s = BookmarkStore::new();
        let html = "<DL><DT><A HREF=\"https://example.com\">Example</A><DT><A HREF=\"https://b.example\">B</A></DL>";
        assert_eq!(s.import_netscape_html(html), 2);
        assert_eq!(s.list()[0].title, "Example");
        assert_eq!(s.list()[1].url, "https://b.example");
    }

    #[test]
    fn json_roundtrip_keeps_bookmarks() {
        let mut s = BookmarkStore::new();
        s.add("A", "https://a.example", "F");
        let json = s.export_json();
        let mut s2 = BookmarkStore::new();
        assert_eq!(s2.import_json(&json).unwrap(), 1);
        assert_eq!(s2.list()[0].title, "A");
    }
}
