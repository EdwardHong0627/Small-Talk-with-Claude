use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset};

use crate::frontmatter::{Meta, take_leading_h1};

/// Fully-resolved article metadata: every field an actual Zola page needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMeta {
    pub title: String,
    pub slug: String,
    pub date: DateTime<FixedOffset>,
    pub tags: Vec<String>,
    pub draft: bool,
    pub canonical_url: Option<String>,
    pub description: Option<String>,
}

/// Resolve frontmatter with fallbacks: a missing title is taken from a
/// leading `# h1` (which is then removed from the body, since the page
/// template renders the title itself); a missing date becomes
/// `default_date` (first publish time, so same-day posts sort correctly
/// and the date doesn't drift on republish).
pub fn resolve(
    meta: Meta,
    body: &str,
    default_date: DateTime<FixedOffset>,
    force_draft: bool,
) -> Result<(ResolvedMeta, String)> {
    let (title, body) = match meta.title {
        Some(title) => {
            // Still drop a duplicated leading h1 if it matches the title.
            match take_leading_h1(body) {
                Some((h1, rest)) if h1 == title => (title, rest),
                _ => (title, body.to_string()),
            }
        }
        None => take_leading_h1(body)
            .context("no title: add `title:` frontmatter or start the article with `# Title`")?,
    };
    let slug = match meta.slug {
        Some(slug) => {
            validate_slug(&slug)?;
            slug
        }
        None => {
            let slug = slugify(&title);
            if slug.is_empty() {
                bail!("title {title:?} produces an empty slug");
            }
            slug
        }
    };
    Ok((
        ResolvedMeta {
            title,
            slug,
            date: meta.date.unwrap_or(default_date),
            tags: meta.tags,
            draft: meta.draft || force_draft,
            canonical_url: meta.canonical_url,
            description: meta.description,
        },
        body,
    ))
}

/// Kebab-case slug: lowercase alphanumerics, everything else collapses to `-`.
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            for lower in c.to_lowercase() {
                slug.push(lower);
            }
        } else {
            pending_dash = true;
        }
    }
    slug
}

/// Reject a hand-written `slug:` frontmatter value that wouldn't round-trip
/// as a clean URL segment: empty, uppercase/non-kebab characters, or a
/// leading/trailing/doubled `-`.
fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        bail!("slug frontmatter is empty");
    }
    let is_kebab = slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !is_kebab
        || slug.starts_with('-')
        || slug.ends_with('-')
        || slug.contains("--")
    {
        bail!(
            "slug {slug:?} must be lowercase kebab-case (letters, digits, single hyphens, no leading/trailing hyphen)"
        );
    }
    Ok(())
}

/// Render a complete Zola page: TOML frontmatter (`+++`) plus body.
pub fn render_page(meta: &ResolvedMeta, body: &str) -> String {
    let mut fm = String::new();
    fm.push_str(&format!("title = {}\n", toml_str(&meta.title)));
    if let Some(description) = &meta.description {
        fm.push_str(&format!("description = {}\n", toml_str(description)));
    }
    // Full offset datetime (a bare TOML value, not a string) so Zola
    // orders same-day posts by publish time.
    fm.push_str(&format!("date = {}\n", meta.date.format("%Y-%m-%dT%H:%M:%S%:z")));
    fm.push_str(&format!("slug = {}\n", toml_str(&meta.slug)));
    if meta.draft {
        fm.push_str("draft = true\n");
    }
    if !meta.tags.is_empty() {
        let tags: Vec<String> = meta.tags.iter().map(|t| toml_str(t)).collect();
        fm.push_str(&format!("\n[taxonomies]\ntags = [{}]\n", tags.join(", ")));
    }
    if let Some(url) = &meta.canonical_url {
        fm.push_str(&format!("\n[extra]\ncanonical_url = {}\n", toml_str(url)));
    }
    format!("+++\n{fm}+++\n\n{}", body.trim_start_matches('\n'))
}

/// Escape a string as a TOML value (proper quoting via the toml crate).
fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-07-10T13:45:00+08:00").unwrap()
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  MCP  vs REST: Design "), "mcp-vs-rest-design");
        assert_eq!(slugify("Rust 2024 — What's New?"), "rust-2024-what-s-new");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn resolve_uses_frontmatter_title() {
        let meta = Meta {
            title: Some("Given Title".into()),
            ..Meta::default()
        };
        let (resolved, body) = resolve(meta, "Body.\n", today(), false).unwrap();
        assert_eq!(resolved.title, "Given Title");
        assert_eq!(resolved.slug, "given-title");
        assert_eq!(resolved.date, today());
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn resolve_uses_frontmatter_slug_override() {
        let meta = Meta {
            title: Some("MCP Is Not a New Paradigm".into()),
            slug: Some("mcp-design-patterns".into()),
            ..Meta::default()
        };
        let (resolved, _) = resolve(meta, "Body.\n", today(), false).unwrap();
        assert_eq!(resolved.slug, "mcp-design-patterns");
    }

    #[test]
    fn resolve_rejects_invalid_slug_override() {
        for bad in ["", "Has Spaces", "Upper-Case", "-leading", "trailing-", "double--dash"] {
            let meta = Meta {
                title: Some("T".into()),
                slug: Some(bad.into()),
                ..Meta::default()
            };
            assert!(
                resolve(meta, "Body.\n", today(), false).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn resolve_falls_back_to_h1_and_strips_it() {
        let (resolved, body) =
            resolve(Meta::default(), "# From H1\n\nBody.\n", today(), false).unwrap();
        assert_eq!(resolved.title, "From H1");
        assert_eq!(body, "\nBody.\n");
    }

    #[test]
    fn resolve_strips_h1_duplicating_frontmatter_title() {
        let meta = Meta {
            title: Some("Same".into()),
            ..Meta::default()
        };
        let (_, body) = resolve(meta, "# Same\n\nBody.\n", today(), false).unwrap();
        assert_eq!(body, "\nBody.\n");
    }

    #[test]
    fn resolve_keeps_distinct_h1() {
        let meta = Meta {
            title: Some("Different".into()),
            ..Meta::default()
        };
        let (_, body) = resolve(meta, "# Original H1\n\nBody.\n", today(), false).unwrap();
        assert_eq!(body, "# Original H1\n\nBody.\n");
    }

    #[test]
    fn resolve_errors_without_any_title() {
        let err = resolve(Meta::default(), "Just prose.\n", today(), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no title"), "unexpected error: {err}");
    }

    #[test]
    fn resolve_force_draft_wins() {
        let (resolved, _) =
            resolve(Meta::default(), "# T\nx\n", today(), true).unwrap();
        assert!(resolved.draft);
    }

    #[test]
    fn render_page_full() {
        let meta = ResolvedMeta {
            title: "A \"Quoted\" Title".into(),
            slug: "a-quoted-title".into(),
            date: today(),
            tags: vec!["rust".into(), "apis".into()],
            draft: true,
            canonical_url: Some("https://example.com/x".into()),
            description: Some("Desc.".into()),
        };
        let page = render_page(&meta, "\nBody.\n");
        // The toml crate picks a literal string ('…') when the value
        // contains double quotes — equally valid TOML for Zola.
        let expected = "+++\n\
            title = 'A \"Quoted\" Title'\n\
            description = \"Desc.\"\n\
            date = 2026-07-10T13:45:00+08:00\n\
            slug = \"a-quoted-title\"\n\
            draft = true\n\
            \n[taxonomies]\ntags = [\"rust\", \"apis\"]\n\
            \n[extra]\ncanonical_url = \"https://example.com/x\"\n\
            +++\n\nBody.\n";
        assert_eq!(page, expected);
    }

    #[test]
    fn render_page_minimal_omits_empty_sections() {
        let meta = ResolvedMeta {
            title: "T".into(),
            slug: "t".into(),
            date: today(),
            tags: vec![],
            draft: false,
            canonical_url: None,
            description: None,
        };
        let page = render_page(&meta, "Body.\n");
        assert!(!page.contains("taxonomies"));
        assert!(!page.contains("extra"));
        assert!(!page.contains("draft"));
        assert!(page.starts_with("+++\ntitle = \"T\"\n"));
        assert!(page.ends_with("+++\n\nBody.\n"));
    }
}
