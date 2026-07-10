use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Article {
    pub slug: String,
    pub title: String,
    pub content_hash: String,
    pub url: String,
    /// When the article was *first* published — preserved across
    /// republishes; it is the default page date, so it must not drift.
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default = "State::current_version")]
    pub version: u32,
    /// Keyed by article source path relative to the workspace root.
    #[serde(default)]
    pub articles: BTreeMap<String, Article>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArticleStatus {
    /// Tracked and content unchanged since last publish.
    Published,
    /// Tracked but source content differs from what was published.
    Changed,
    /// Not in the state file.
    Untracked,
}

impl State {
    fn current_version() -> u32 {
        1
    }

    /// Missing file is an empty state, not an error.
    pub fn load(path: &Path) -> Result<State> {
        if !path.exists() {
            return Ok(State {
                version: Self::current_version(),
                ..State::default()
            });
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }

    /// Atomic write: temp file in the same directory, then rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("state path has no parent")?;
        let json = serde_json::to_string_pretty(self)?;
        let tmp = dir.join(".mdpub-state.json.tmp");
        std::fs::write(&tmp, json.as_bytes())
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("renaming into {}", path.display()))?;
        Ok(())
    }

    pub fn status(&self, key: &str, current_hash: &str) -> ArticleStatus {
        match self.articles.get(key) {
            None => ArticleStatus::Untracked,
            Some(article) if article.content_hash == current_hash => ArticleStatus::Published,
            Some(_) => ArticleStatus::Changed,
        }
    }

    /// The source path (if any) that already owns `slug`, other than `key`.
    pub fn slug_owner(&self, slug: &str, key: &str) -> Option<&str> {
        self.articles
            .iter()
            .find(|(k, a)| a.slug == slug && k.as_str() != key)
            .map(|(k, _)| k.as_str())
    }
}

pub fn hash_content(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(slug: &str, hash: &str) -> Article {
        Article {
            slug: slug.into(),
            title: slug.into(),
            content_hash: hash.into(),
            url: format!("https://example.com/blog/{slug}/"),
            published_at: Utc::now(),
        }
    }

    #[test]
    fn load_missing_file_gives_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(state.version, 1);
        assert!(state.articles.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut state = State::load(&path).unwrap();
        state
            .articles
            .insert("Day1/foo.md".into(), article("foo", "blake3:abc"));
        state.save(&path).unwrap();
        let reloaded = State::load(&path).unwrap();
        assert_eq!(reloaded.articles["Day1/foo.md"], state.articles["Day1/foo.md"]);
        // No leftover temp file from the atomic write.
        assert!(!dir.path().join(".mdpub-state.json.tmp").exists());
    }

    #[test]
    fn corrupt_state_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(State::load(&path).is_err());
    }

    #[test]
    fn status_detects_changes() {
        let mut state = State::default();
        state
            .articles
            .insert("a.md".into(), article("a", "blake3:old"));
        assert_eq!(state.status("a.md", "blake3:old"), ArticleStatus::Published);
        assert_eq!(state.status("a.md", "blake3:new"), ArticleStatus::Changed);
        assert_eq!(state.status("b.md", "blake3:x"), ArticleStatus::Untracked);
    }

    #[test]
    fn slug_owner_finds_collisions() {
        let mut state = State::default();
        state
            .articles
            .insert("a.md".into(), article("shared-slug", "h"));
        assert_eq!(state.slug_owner("shared-slug", "b.md"), Some("a.md"));
        assert_eq!(state.slug_owner("shared-slug", "a.md"), None);
        assert_eq!(state.slug_owner("other", "b.md"), None);
    }

    #[test]
    fn hash_is_stable_and_prefixed() {
        let h = hash_content(b"hello");
        assert!(h.starts_with("blake3:"));
        assert_eq!(h, hash_content(b"hello"));
        assert_ne!(h, hash_content(b"hello!"));
    }
}
