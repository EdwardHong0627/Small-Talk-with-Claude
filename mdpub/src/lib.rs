pub mod cli;
pub mod config;
pub mod deploy;
pub mod frontmatter;
pub mod images;
pub mod runner;
pub mod state;
pub mod zola;

use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{Local, Utc};

use cli::{Cli, Command};
use config::{CONFIG_FILE, Workspace};
use runner::Runner;
use state::{ArticleStatus, State};

/// Exit code for "already published and unchanged" (use --force to override).
pub const EXIT_UNCHANGED: i32 = 2;

pub fn run(cli: Cli, runner: &mut dyn Runner, cwd: &Path) -> Result<i32> {
    match cli.command {
        Command::Init { server, base_url, docroot, site_dir } => {
            init(cwd, &server, &base_url, &docroot, &site_dir)
        }
        Command::Publish { file, dry_run, force, draft } => {
            publish(runner, cwd, &file, dry_run, force, draft)
        }
        Command::Preview { no_open } => preview(runner, cwd, no_open),
        Command::Status => status(cwd),
        Command::Unpublish { file } => unpublish(runner, cwd, &file),
    }
}

fn init(cwd: &Path, server: &str, base_url: &str, docroot: &str, site_dir: &Path) -> Result<i32> {
    let path = cwd.join(CONFIG_FILE);
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    let config = config::Config {
        base_url: base_url.trim_end_matches('/').to_string(),
        server: server.to_string(),
        docroot: docroot.to_string(),
        site_dir: site_dir.to_path_buf(),
        zola_bin: "zola".into(),
        rsync_bin: "rsync".into(),
    };
    std::fs::write(&path, toml::to_string_pretty(&config)?)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(0)
}

fn publish(
    runner: &mut dyn Runner,
    cwd: &Path,
    file: &Path,
    dry_run: bool,
    force: bool,
    draft: bool,
) -> Result<i32> {
    let ws = Workspace::discover(cwd)?;
    let source = cwd.join(file);
    let raw = std::fs::read(&source)
        .with_context(|| format!("reading {}", source.display()))?;
    let text = String::from_utf8(raw.clone())
        .with_context(|| format!("{} is not valid UTF-8", source.display()))?;

    let key = article_key(&ws.root, &source)?;
    let mut st = State::load(&ws.state_path())?;

    let parsed = frontmatter::parse(&text)?;
    let (meta, body) = zola::resolve(parsed.meta, &parsed.body, Local::now().date_naive(), draft)?;

    let source_dir = source
        .parent()
        .with_context(|| format!("{} has no parent directory", source.display()))?;
    let (body, local_images) = images::rewrite(&body, source_dir)?;

    // The hash covers the article and every referenced local image, so
    // editing an image counts as a content change.
    let mut hash_input = raw;
    for image in &local_images {
        hash_input.extend(
            std::fs::read(&image.source)
                .with_context(|| format!("reading {}", image.source.display()))?,
        );
    }
    let hash = state::hash_content(&hash_input);

    if !force && !dry_run && st.status(&key, &hash) == ArticleStatus::Published {
        let url = &st.articles[&key].url;
        eprintln!("unchanged since last publish ({url}) — use --force to republish");
        return Ok(EXIT_UNCHANGED);
    }

    if let Some(owner) = st.slug_owner(&meta.slug, &key) {
        bail!(
            "slug {:?} is already used by {owner} — retitle one of the articles",
            meta.slug
        );
    }

    // A retitled article gets a new slug; drop the page written under the
    // old one so the deployed site (rsync --delete) doesn't keep it.
    if let Some(previous) = st.articles.get(&key)
        && previous.slug != meta.slug
    {
        remove_page(&ws.content_dir(), &previous.slug)?;
    }

    // Colocated page: content/blog/<slug>/index.md plus its images.
    // Recreate the directory from scratch so removed images don't linger.
    let content_dir = ws.content_dir();
    remove_page(&content_dir, &meta.slug)?;
    let page_dir = content_dir.join(&meta.slug);
    std::fs::create_dir_all(&page_dir)
        .with_context(|| format!("creating {}", page_dir.display()))?;
    std::fs::write(page_dir.join("index.md"), zola::render_page(&meta, &body))
        .with_context(|| format!("writing {}", page_dir.join("index.md").display()))?;
    for image in &local_images {
        std::fs::copy(&image.source, page_dir.join(&image.file_name))
            .with_context(|| format!("copying {}", image.source.display()))?;
    }

    deploy::build(runner, &ws)?;

    let url = ws.article_url(&meta.slug);
    println!("  Title:  {}", meta.title);
    println!("  Slug:   {}", meta.slug);
    println!("  Date:   {}", meta.date);
    println!(
        "  Tags:   {}",
        if meta.tags.is_empty() { "(none)".to_string() } else { meta.tags.join(", ") }
    );
    if !local_images.is_empty() {
        println!("  Images: {}", local_images.len());
    }
    if meta.draft {
        println!("  Draft:  yes — page is not rendered until published without --draft");
    }

    if dry_run {
        println!("  Dry run — site built locally, nothing deployed.");
        return Ok(0);
    }

    deploy::deploy(runner, &ws)?;
    st.articles.insert(
        key,
        state::Article {
            slug: meta.slug.clone(),
            title: meta.title.clone(),
            content_hash: hash,
            url: url.clone(),
            published_at: Utc::now(),
        },
    );
    st.save(&ws.state_path())?;
    println!("  Live:   {url}");
    Ok(0)
}

fn preview(runner: &mut dyn Runner, cwd: &Path, no_open: bool) -> Result<i32> {
    let ws = Workspace::discover(cwd)?;
    if !no_open {
        // zola serve blocks, so open the browser first; the page appears
        // as soon as the server is up (zola serve is near-instant).
        let _ = open::that("http://127.0.0.1:1111");
    }
    runner.run(&ws.config.zola_bin, &["serve".into()], Some(&ws.site_dir()))?;
    Ok(0)
}

fn status(cwd: &Path) -> Result<i32> {
    let ws = Workspace::discover(cwd)?;
    let st = State::load(&ws.state_path())?;
    if st.articles.is_empty() {
        println!("No published articles tracked yet.");
        return Ok(0);
    }
    for (key, article) in &st.articles {
        let source = ws.root.join(key);
        let label = match std::fs::read(&source) {
            Err(_) => "missing source",
            Ok(bytes) => match st.status(key, &state::hash_content(&bytes)) {
                ArticleStatus::Published => "published",
                ArticleStatus::Changed => "changed since publish",
                ArticleStatus::Untracked => unreachable!("key comes from state"),
            },
        };
        println!("{key}  [{label}]  {}", article.url);
    }
    Ok(0)
}

fn unpublish(runner: &mut dyn Runner, cwd: &Path, file: &Path) -> Result<i32> {
    let ws = Workspace::discover(cwd)?;
    let source = cwd.join(file);
    let key = article_key(&ws.root, &source)?;
    let mut st = State::load(&ws.state_path())?;
    let Some(article) = st.articles.get(&key) else {
        bail!("{key} is not tracked as published");
    };
    remove_page(&ws.content_dir(), &article.slug)?;
    deploy::build(runner, &ws)?;
    deploy::deploy(runner, &ws)?;
    st.articles.remove(&key);
    st.save(&ws.state_path())?;
    println!("Unpublished {key}");
    Ok(0)
}

/// Remove a page in either layout: colocated `<slug>/` directory or the
/// flat `<slug>.md` file written by earlier versions.
fn remove_page(content_dir: &Path, slug: &str) -> Result<()> {
    let flat = content_dir.join(format!("{slug}.md"));
    if flat.exists() {
        std::fs::remove_file(&flat)
            .with_context(|| format!("removing {}", flat.display()))?;
    }
    let dir = content_dir.join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

/// State-file key: source path relative to the workspace root, with `/`
/// separators. Falls back to the absolute path for files outside the root.
fn article_key(root: &Path, file: &Path) -> Result<String> {
    let file = file
        .canonicalize()
        .with_context(|| format!("resolving {}", file.display()))?;
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let rel = file.strip_prefix(&root).unwrap_or(&file);
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MockRunner;

    /// Build a workspace on disk: mdpub.toml + site skeleton + one article.
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(CONFIG_FILE),
            "base_url = \"https://blog.example.com\"\n\
             server = \"deploy@203.0.113.7\"\n\
             docroot = \"/var/www/blog\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("blog/content/blog")).unwrap();
        let article = dir.path().join("post.md");
        std::fs::write(&article, "# Test Article\n\nHello.\n").unwrap();
        (dir, article)
    }

    fn publish_cmd(file: &Path, dry_run: bool, force: bool) -> Cli {
        Cli {
            command: Command::Publish {
                file: file.to_path_buf(),
                dry_run,
                force,
                draft: false,
            },
        }
    }

    #[test]
    fn publish_writes_page_builds_deploys_and_records() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        let code = run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);

        let page = dir.path().join("blog/content/blog/test-article/index.md");
        let contents = std::fs::read_to_string(&page).unwrap();
        assert!(contents.contains("title = \"Test Article\""));
        assert!(contents.contains("slug = \"test-article\""));

        assert_eq!(mock.calls.len(), 2);
        assert_eq!(mock.calls[0].args[0], "build");
        assert_eq!(mock.calls[1].program, "rsync");

        let st = State::load(&dir.path().join(config::STATE_FILE)).unwrap();
        let entry = &st.articles["post.md"];
        assert_eq!(entry.slug, "test-article");
        assert_eq!(entry.url, "https://blog.example.com/blog/test-article/");
    }

    #[test]
    fn dry_run_builds_but_does_not_deploy_or_record() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        let code = run(publish_cmd(&article, true, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);
        assert_eq!(mock.calls.len(), 1, "only zola build expected");
        assert!(!dir.path().join(config::STATE_FILE).exists());
    }

    #[test]
    fn unchanged_republish_exits_2_and_force_overrides() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();

        let code = run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, EXIT_UNCHANGED);
        assert_eq!(mock.calls.len(), 2, "no new build/deploy on unchanged");

        let code = run(publish_cmd(&article, false, true), &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);
        assert_eq!(mock.calls.len(), 4);
    }

    #[test]
    fn changed_content_republishes_without_force() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        std::fs::write(&article, "# Test Article\n\nEdited.\n").unwrap();
        let code = run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn retitled_article_removes_stale_page() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert!(dir.path().join("blog/content/blog/test-article/index.md").exists());

        std::fs::write(&article, "# New Name\n\nHello.\n").unwrap();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert!(!dir.path().join("blog/content/blog/test-article").exists());
        assert!(dir.path().join("blog/content/blog/new-name/index.md").exists());
    }

    #[test]
    fn slug_collision_with_other_article_errors() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();

        let other = dir.path().join("other.md");
        std::fs::write(&other, "# Test Article\n\nDifferent file, same title.\n").unwrap();
        let err = run(publish_cmd(&other, false, false), &mut mock, dir.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("already used by post.md"), "unexpected: {err}");
    }

    #[test]
    fn draft_flag_marks_page_as_draft() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        let cli = Cli {
            command: Command::Publish { file: article, dry_run: true, force: false, draft: true },
        };
        run(cli, &mut mock, dir.path()).unwrap();
        let page = std::fs::read_to_string(dir.path().join("blog/content/blog/test-article/index.md")).unwrap();
        assert!(page.contains("draft = true"));
    }

    #[test]
    fn unpublish_removes_page_and_state() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();

        let cli = Cli { command: Command::Unpublish { file: article } };
        let code = run(cli, &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);
        assert!(!dir.path().join("blog/content/blog/test-article").exists());
        let st = State::load(&dir.path().join(config::STATE_FILE)).unwrap();
        assert!(st.articles.is_empty());
        // build + deploy ran for the removal too
        assert_eq!(mock.calls.len(), 4);
    }

    #[test]
    fn unpublish_untracked_errors() {
        let (dir, article) = fixture();
        let mut mock = MockRunner::default();
        let cli = Cli { command: Command::Unpublish { file: article } };
        let err = run(cli, &mut mock, dir.path()).unwrap_err().to_string();
        assert!(err.contains("not tracked"));
    }

    #[test]
    fn publish_copies_referenced_images_next_to_page() {
        let (dir, article) = fixture();
        std::fs::write(dir.path().join("shot.png"), b"png-bytes").unwrap();
        std::fs::write(&article, "# Test Article\n\n![screenshot](shot.png)\n").unwrap();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();

        let page_dir = dir.path().join("blog/content/blog/test-article");
        assert!(page_dir.join("shot.png").exists());
        let page = std::fs::read_to_string(page_dir.join("index.md")).unwrap();
        assert!(page.contains("![screenshot](shot.png)"));
    }

    #[test]
    fn editing_only_the_image_counts_as_changed() {
        let (dir, article) = fixture();
        std::fs::write(dir.path().join("shot.png"), b"v1").unwrap();
        std::fs::write(&article, "# Test Article\n\n![s](shot.png)\n").unwrap();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();

        // Unchanged image + text → exit 2.
        let code = run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, EXIT_UNCHANGED);

        // New image bytes alone → republish allowed.
        std::fs::write(dir.path().join("shot.png"), b"v2").unwrap();
        let code = run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn removed_image_disappears_from_page_dir() {
        let (dir, article) = fixture();
        std::fs::write(dir.path().join("shot.png"), b"png").unwrap();
        std::fs::write(&article, "# Test Article\n\n![s](shot.png)\n").unwrap();
        let mut mock = MockRunner::default();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        let copied = dir.path().join("blog/content/blog/test-article/shot.png");
        assert!(copied.exists());

        std::fs::write(&article, "# Test Article\n\nNo image now.\n").unwrap();
        run(publish_cmd(&article, false, false), &mut mock, dir.path()).unwrap();
        assert!(!copied.exists());
    }

    #[test]
    fn missing_image_fails_before_any_build() {
        let (dir, article) = fixture();
        std::fs::write(&article, "# Test Article\n\n![gone](nope.png)\n").unwrap();
        let mut mock = MockRunner::default();
        let err = run(publish_cmd(&article, false, false), &mut mock, dir.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope.png"), "unexpected: {err}");
        assert!(mock.calls.is_empty(), "no build/deploy on failure");
    }

    #[test]
    fn init_writes_config_once() {
        let dir = tempfile::tempdir().unwrap();
        let cli = || Cli {
            command: Command::Init {
                server: "deploy@1.2.3.4".into(),
                base_url: "https://blog.example.com/".into(),
                docroot: "/var/www/blog".into(),
                site_dir: "blog".into(),
            },
        };
        let mut mock = MockRunner::default();
        assert_eq!(run(cli(), &mut mock, dir.path()).unwrap(), 0);
        let written = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(written.contains("base_url = \"https://blog.example.com\""), "trailing slash trimmed");
        assert!(run(cli(), &mut mock, dir.path()).is_err(), "refuses to overwrite");
    }
}
