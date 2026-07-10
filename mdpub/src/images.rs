use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// A local image referenced by the article, to be colocated with the page.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalImage {
    /// Resolved path of the image on disk.
    pub source: PathBuf,
    /// File name inside the page directory (unique per article).
    pub file_name: String,
}

/// Rewrite markdown image references to page-relative file names and
/// collect the local files to copy next to the page (Zola colocated
/// assets). Remote (`http://`, `https://`, `data:`) and site-absolute
/// (`/...`) targets are left untouched. Fenced code blocks are skipped.
pub fn rewrite(body: &str, source_dir: &Path) -> Result<(String, Vec<LocalImage>)> {
    let mut out = String::with_capacity(body.len());
    let mut images: Vec<LocalImage> = Vec::new();
    let mut names: HashMap<String, PathBuf> = HashMap::new();
    let mut in_fence = false;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
        } else {
            out.push_str(&rewrite_line(line, source_dir, &mut images, &mut names)?);
        }
    }
    Ok((out, images))
}

fn rewrite_line(
    line: &str,
    source_dir: &Path,
    images: &mut Vec<LocalImage>,
    names: &mut HashMap<String, PathBuf>,
) -> Result<String> {
    let mut result = String::new();
    let mut rest = line;
    loop {
        // An image reference is `![alt](target)`.
        let Some(bang) = rest.find("![") else { break };
        let Some(paren_open) = rest[bang..].find("](").map(|i| bang + i) else { break };
        let Some(paren_close) = rest[paren_open..].find(')').map(|i| paren_open + i) else {
            break;
        };

        let target = rest[paren_open + 2..paren_close].trim();
        // `![alt](path "title")` — the title stays with the reference.
        let (path_part, title_part) = match target.find(char::is_whitespace) {
            Some(i) => (&target[..i], &target[i..]),
            None => (target, ""),
        };

        result.push_str(&rest[..paren_open + 2]);
        if is_external(path_part) {
            result.push_str(target);
        } else {
            let source = source_dir.join(path_part);
            if !source.is_file() {
                bail!(
                    "image {path_part:?} not found at {} — fix the reference or the file",
                    source.display()
                );
            }
            let desired = source
                .file_name()
                .context("image path has no file name")?
                .to_string_lossy()
                .into_owned();
            let file_name = unique_name(&desired, &source, names);
            if !images.iter().any(|i| i.file_name == file_name) {
                images.push(LocalImage { source: source.clone(), file_name: file_name.clone() });
            }
            result.push_str(&file_name);
            result.push_str(title_part);
        }
        result.push(')');
        rest = &rest[paren_close + 1..];
    }
    result.push_str(rest);
    Ok(result)
}

fn is_external(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("data:")
        || target.starts_with('/')
}

/// Same source keeps its name; a different source with a clashing name
/// gets `-2`, `-3`, … appended to the stem.
fn unique_name(desired: &str, source: &Path, names: &mut HashMap<String, PathBuf>) -> String {
    if let Some(existing) = names.get(desired) {
        if existing == source {
            return desired.to_string();
        }
    } else {
        names.insert(desired.to_string(), source.to_path_buf());
        return desired.to_string();
    }
    let (stem, ext) = match desired.rfind('.') {
        Some(i) => (&desired[..i], &desired[i..]),
        None => (desired, ""),
    };
    for n in 2.. {
        let candidate = format!("{stem}-{n}{ext}");
        match names.get(&candidate) {
            Some(existing) if existing != source => continue,
            Some(_) => return candidate,
            None => {
                names.insert(candidate.clone(), source.to_path_buf());
                return candidate;
            }
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for f in files {
            let path = dir.path().join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"png-bytes").unwrap();
        }
        dir
    }

    #[test]
    fn local_image_is_rewritten_and_collected() {
        let dir = setup(&["manuscript.png"]);
        let (body, images) =
            rewrite("Intro\n\n![The plan](manuscript.png)\n", dir.path()).unwrap();
        assert_eq!(body, "Intro\n\n![The plan](manuscript.png)\n");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].file_name, "manuscript.png");
        assert_eq!(images[0].source, dir.path().join("manuscript.png"));
    }

    #[test]
    fn relative_subdir_flattens_to_file_name() {
        let dir = setup(&["assets/diagram.png"]);
        let (body, images) =
            rewrite("![d](assets/diagram.png)\n", dir.path()).unwrap();
        assert_eq!(body, "![d](diagram.png)\n");
        assert_eq!(images[0].source, dir.path().join("assets/diagram.png"));
    }

    #[test]
    fn title_is_preserved() {
        let dir = setup(&["a.png"]);
        let (body, _) = rewrite("![x](a.png \"A title\")\n", dir.path()).unwrap();
        assert_eq!(body, "![x](a.png \"A title\")\n");
    }

    #[test]
    fn external_and_absolute_targets_untouched() {
        let dir = setup(&[]);
        let src = "![r](https://x.com/i.png) ![p](/already/on/site.png) ![d](data:image/png;base64,AA==)\n";
        let (body, images) = rewrite(src, dir.path()).unwrap();
        assert_eq!(body, src);
        assert!(images.is_empty());
    }

    #[test]
    fn missing_local_image_errors() {
        let dir = setup(&[]);
        let err = rewrite("![x](nope.png)\n", dir.path()).unwrap_err().to_string();
        assert!(err.contains("nope.png"), "unexpected: {err}");
    }

    #[test]
    fn fenced_code_blocks_are_skipped() {
        let dir = setup(&[]);
        let src = "```md\n![example](not-a-real-file.png)\n```\n";
        let (body, images) = rewrite(src, dir.path()).unwrap();
        assert_eq!(body, src);
        assert!(images.is_empty());
    }

    #[test]
    fn name_collision_from_different_dirs_gets_suffix() {
        let dir = setup(&["a/pic.png", "b/pic.png"]);
        let (body, images) =
            rewrite("![1](a/pic.png)\n![2](b/pic.png)\n", dir.path()).unwrap();
        assert_eq!(body, "![1](pic.png)\n![2](pic-2.png)\n");
        assert_eq!(images.len(), 2);
        assert_eq!(images[1].file_name, "pic-2.png");
    }

    #[test]
    fn same_image_twice_is_copied_once() {
        let dir = setup(&["pic.png"]);
        let (_, images) =
            rewrite("![1](pic.png)\n![again](pic.png)\n", dir.path()).unwrap();
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn multiple_images_on_one_line() {
        let dir = setup(&["a.png", "b.png"]);
        let (body, images) = rewrite("![a](a.png) and ![b](b.png)\n", dir.path()).unwrap();
        assert_eq!(body, "![a](a.png) and ![b](b.png)\n");
        assert_eq!(images.len(), 2);
    }
}
