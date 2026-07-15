use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "mdpub.toml";
pub const STATE_FILE: &str = ".mdpub-state.json";

fn default_site_dir() -> PathBuf {
    PathBuf::from("blog")
}

fn default_zola_bin() -> String {
    "zola".into()
}

fn default_rsync_bin() -> String {
    "rsync".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Public base URL of the blog, e.g. "https://blog.example.com"
    pub base_url: String,
    /// rsync/SSH target host, e.g. "deploy@203.0.113.7"
    pub server: String,
    /// Absolute docroot on the server, e.g. "/var/www/blog"
    pub docroot: String,
    /// Zola site directory, relative to this config file
    #[serde(default = "default_site_dir")]
    pub site_dir: PathBuf,
    #[serde(default = "default_zola_bin")]
    pub zola_bin: String,
    #[serde(default = "default_rsync_bin")]
    pub rsync_bin: String,
}

/// A loaded config plus the directory it was found in (the repo root:
/// state file lives there and article paths are stored relative to it).
#[derive(Debug, Clone)]
pub struct Workspace {
    pub config: Config,
    pub root: PathBuf,
}

impl Workspace {
    /// Walk up from `start` until a directory containing mdpub.toml is found.
    pub fn discover(start: &Path) -> Result<Workspace> {
        let mut dir = start.to_path_buf();
        loop {
            let candidate = dir.join(CONFIG_FILE);
            if candidate.is_file() {
                let config = Config::load(&candidate)?;
                return Ok(Workspace { config, root: dir });
            }
            if !dir.pop() {
                bail!(
                    "no {CONFIG_FILE} found in {} or any parent directory — run `mdpub init` first",
                    start.display()
                );
            }
        }
    }

    pub fn site_dir(&self) -> PathBuf {
        self.root.join(&self.config.site_dir)
    }

    pub fn content_dir(&self) -> PathBuf {
        self.site_dir().join("content").join("blog")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join(STATE_FILE)
    }

    /// URL of a published article for a given slug.
    pub fn article_url(&self, slug: &str) -> String {
        format!(
            "{}/blog/{}/",
            self.config.base_url.trim_end_matches('/'),
            slug
        )
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        if config.base_url.is_empty() || config.server.is_empty() || config.docroot.is_empty() {
            bail!(
                "{}: base_url, server, and docroot must all be set",
                path.display()
            );
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    const MINIMAL: &str = r#"
base_url = "https://blog.example.com"
server = "deploy@203.0.113.7"
docroot = "/var/www/blog"
"#;

    #[test]
    fn load_minimal_applies_defaults() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, MINIMAL);
        let config = Config::load(&dir.path().join(CONFIG_FILE)).unwrap();
        assert_eq!(config.site_dir, PathBuf::from("blog"));
        assert_eq!(config.zola_bin, "zola");
        assert_eq!(config.rsync_bin, "rsync");
    }

    #[test]
    fn load_rejects_missing_required_field() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, "base_url = \"https://x\"\n");
        assert!(Config::load(&dir.path().join(CONFIG_FILE)).is_err());
    }

    #[test]
    fn discover_walks_up_to_config() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, MINIMAL);
        let nested = dir.path().join("Day1/sub");
        std::fs::create_dir_all(&nested).unwrap();
        let ws = Workspace::discover(&nested).unwrap();
        assert_eq!(ws.root, dir.path());
        assert_eq!(ws.content_dir(), dir.path().join("blog/content/blog"));
    }

    #[test]
    fn discover_fails_without_config() {
        let dir = tempfile::tempdir().unwrap();
        let err = Workspace::discover(dir.path()).unwrap_err().to_string();
        assert!(err.contains("mdpub init"), "unexpected error: {err}");
    }

    #[test]
    fn article_url_joins_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE, MINIMAL);
        let ws = Workspace::discover(dir.path()).unwrap();
        assert_eq!(
            ws.article_url("my-post"),
            "https://blog.example.com/blog/my-post/"
        );
    }
}
