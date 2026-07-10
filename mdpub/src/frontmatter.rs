use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;

/// Metadata declared in a source article's optional YAML frontmatter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub date: Option<NaiveDate>,
    pub draft: bool,
    pub canonical_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub meta: Meta,
    pub body: String,
}

/// Tags may be a YAML list (`tags: [a, b]`) or a comma-separated string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TagList {
    List(Vec<String>),
    One(String),
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMeta {
    title: Option<String>,
    tags: Option<TagList>,
    date: Option<String>,
    draft: Option<bool>,
    canonical_url: Option<String>,
    description: Option<String>,
}

/// Split an article into YAML frontmatter (if any) and markdown body.
/// Frontmatter must start on line 1 with `---` and end with `---` or `...`.
pub fn parse(source: &str) -> Result<Parsed> {
    let Some(yaml_and_rest) = strip_open_fence(source) else {
        return Ok(Parsed {
            meta: Meta::default(),
            body: source.to_string(),
        });
    };

    let mut yaml = String::new();
    let mut lines = yaml_and_rest.split_inclusive('\n');
    for line in &mut lines {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            let body: String = lines.collect();
            let raw: RawMeta = if yaml.trim().is_empty() {
                RawMeta::default()
            } else {
                serde_yaml_ng::from_str(&yaml).context("parsing YAML frontmatter")?
            };
            return Ok(Parsed {
                meta: resolve_raw(raw)?,
                body,
            });
        }
        yaml.push_str(line);
    }
    bail!("frontmatter opened with `---` on line 1 but never closed");
}

/// If the body starts with an ATX h1 (`# Title`) as its first non-blank
/// line, return the title and the body with that heading removed.
pub fn take_leading_h1(body: &str) -> Option<(String, String)> {
    let mut rest = body;
    let mut consumed = 0;
    loop {
        let line_end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
        let (line, tail) = rest.split_at(line_end);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if tail.is_empty() {
                return None;
            }
            consumed += line_end;
            rest = tail;
            continue;
        }
        let title = trimmed.strip_prefix("# ")?.trim();
        if title.is_empty() {
            return None;
        }
        let mut remainder = String::with_capacity(body.len());
        remainder.push_str(&body[..consumed]);
        remainder.push_str(tail);
        return Some((title.to_string(), remainder));
    }
}

fn strip_open_fence(source: &str) -> Option<&str> {
    source
        .strip_prefix("---\r\n")
        .or_else(|| source.strip_prefix("---\n"))
}

fn resolve_raw(raw: RawMeta) -> Result<Meta> {
    let tags = match raw.tags {
        None => Vec::new(),
        Some(TagList::List(list)) => list,
        Some(TagList::One(s)) => s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
    };
    let date = raw
        .date
        .map(|d| parse_date(&d))
        .transpose()?;
    Ok(Meta {
        title: raw.title,
        tags,
        date,
        draft: raw.draft.unwrap_or(false),
        canonical_url: raw.canonical_url,
        description: raw.description,
    })
}

/// Accept `YYYY-MM-DD` or any RFC 3339 datetime.
fn parse_date(value: &str) -> Result<NaiveDate> {
    let value = value.trim();
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(dt.date_naive());
    }
    bail!("invalid date {value:?} — expected YYYY-MM-DD or RFC 3339");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_returns_default_meta() {
        let parsed = parse("# Hello\n\nBody.\n").unwrap();
        assert_eq!(parsed.meta, Meta::default());
        assert_eq!(parsed.body, "# Hello\n\nBody.\n");
    }

    #[test]
    fn full_frontmatter_is_parsed() {
        let src = "---\n\
                   title: My Post\n\
                   tags: [rust, apis]\n\
                   date: 2026-07-10\n\
                   draft: true\n\
                   canonical_url: https://example.com/x\n\
                   description: A post.\n\
                   ---\n\
                   Body text.\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.meta.title.as_deref(), Some("My Post"));
        assert_eq!(parsed.meta.tags, vec!["rust", "apis"]);
        assert_eq!(
            parsed.meta.date,
            Some(NaiveDate::from_ymd_opt(2026, 7, 10).unwrap())
        );
        assert!(parsed.meta.draft);
        assert_eq!(
            parsed.meta.canonical_url.as_deref(),
            Some("https://example.com/x")
        );
        assert_eq!(parsed.body, "Body text.\n");
    }

    #[test]
    fn comma_separated_tags_are_split() {
        let src = "---\ntags: rust, apis , llm\n---\nx\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.meta.tags, vec!["rust", "apis", "llm"]);
    }

    #[test]
    fn crlf_frontmatter_is_handled() {
        let src = "---\r\ntitle: Windows Post\r\n---\r\nBody.\r\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.meta.title.as_deref(), Some("Windows Post"));
        assert_eq!(parsed.body, "Body.\r\n");
    }

    #[test]
    fn thematic_break_in_body_is_not_frontmatter() {
        // No opening fence on line 1, so the later `---` is just markdown.
        let src = "intro\n\n---\n\nmore\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.meta, Meta::default());
        assert_eq!(parsed.body, src);
    }

    #[test]
    fn body_after_close_may_contain_dashes() {
        let src = "---\ntitle: T\n---\nabove\n\n---\n\nbelow\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.body, "above\n\n---\n\nbelow\n");
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        assert!(parse("---\ntitle: T\nno close\n").is_err());
    }

    #[test]
    fn invalid_yaml_errors() {
        assert!(parse("---\ntitle: [unclosed\n---\nx\n").is_err());
    }

    #[test]
    fn unknown_frontmatter_key_errors() {
        assert!(parse("---\ntitel: typo\n---\nx\n").is_err());
    }

    #[test]
    fn rfc3339_date_is_accepted() {
        let src = "---\ndate: 2026-07-10T08:00:00Z\n---\nx\n";
        let parsed = parse(src).unwrap();
        assert_eq!(
            parsed.meta.date,
            Some(NaiveDate::from_ymd_opt(2026, 7, 10).unwrap())
        );
    }

    #[test]
    fn bad_date_errors() {
        assert!(parse("---\ndate: July 10th\n---\nx\n").is_err());
    }

    #[test]
    fn take_leading_h1_extracts_and_removes_title() {
        let (title, rest) = take_leading_h1("\n# The Title\n\nBody.\n").unwrap();
        assert_eq!(title, "The Title");
        assert_eq!(rest, "\n\nBody.\n");
    }

    #[test]
    fn take_leading_h1_requires_h1_first() {
        assert!(take_leading_h1("Intro para\n\n# Later\n").is_none());
        assert!(take_leading_h1("## Not h1\n").is_none());
        assert!(take_leading_h1("").is_none());
    }
}
