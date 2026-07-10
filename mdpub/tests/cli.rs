//! End-to-end CLI tests against a temp workspace. External commands
//! (zola, rsync) are replaced by stub scripts that log their arguments,
//! so no network or real site build is involved.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

struct Fixture {
    dir: tempfile::TempDir,
    zola_log: PathBuf,
    rsync_log: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let zola_log = dir.path().join("zola.log");
        let rsync_log = dir.path().join("rsync.log");
        let zola_bin = stub(&bin, "zola", &zola_log);
        let rsync_bin = stub(&bin, "rsync", &rsync_log);

        fs::write(
            dir.path().join("mdpub.toml"),
            format!(
                "base_url = \"https://blog.example.com\"\n\
                 server = \"deploy@203.0.113.7\"\n\
                 docroot = \"/var/www/blog\"\n\
                 zola_bin = \"{}\"\n\
                 rsync_bin = \"{}\"\n",
                zola_bin.display(),
                rsync_bin.display()
            ),
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("blog/content/blog")).unwrap();
        fs::create_dir_all(dir.path().join("Day1")).unwrap();
        fs::write(
            dir.path().join("Day1/post.md"),
            "---\ntitle: Integration Post\ntags: [testing]\n---\nSome **body**.\n",
        )
        .unwrap();
        Fixture { dir, zola_log, rsync_log }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("mdpub").unwrap();
        cmd.current_dir(self.dir.path()).args(args);
        cmd
    }

    fn log(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }
}

/// A stub executable that appends its arguments to a log file.
fn stub(bin_dir: &Path, name: &str, log: &Path) -> PathBuf {
    let path = bin_dir.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", log.display()),
    )
    .unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

#[test]
fn publish_dry_run_builds_without_deploying() {
    let fx = Fixture::new();
    fx.cmd(&["publish", "Day1/post.md", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Integration Post"))
        .stdout(predicate::str::contains("Dry run"));

    assert!(fx.log(&fx.zola_log).contains("build --base-url https://blog.example.com"));
    assert_eq!(fx.log(&fx.rsync_log), "", "rsync must not run on dry run");
    assert!(fx.dir.path().join("blog/content/blog/integration-post/index.md").exists());
    assert!(!fx.dir.path().join(".mdpub-state.json").exists());
}

#[test]
fn publish_deploys_and_reports_url() {
    let fx = Fixture::new();
    fx.cmd(&["publish", "Day1/post.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "https://blog.example.com/blog/integration-post/",
        ));

    let rsync = fx.log(&fx.rsync_log);
    assert!(rsync.contains("-az --delete"), "rsync args: {rsync}");
    assert!(rsync.contains("deploy@203.0.113.7:/var/www/blog/"), "rsync args: {rsync}");
    assert!(fx.dir.path().join(".mdpub-state.json").exists());
}

#[test]
fn unchanged_republish_exits_2_until_forced() {
    let fx = Fixture::new();
    fx.cmd(&["publish", "Day1/post.md"]).assert().success();
    fx.cmd(&["publish", "Day1/post.md"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--force"));
    fx.cmd(&["publish", "Day1/post.md", "--force"]).assert().success();
}

#[test]
fn status_reflects_content_changes() {
    let fx = Fixture::new();
    fx.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No published articles"));

    fx.cmd(&["publish", "Day1/post.md"]).assert().success();
    fx.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Day1/post.md  [published]"));

    fs::write(
        fx.dir.path().join("Day1/post.md"),
        "---\ntitle: Integration Post\n---\nEdited body.\n",
    )
    .unwrap();
    fx.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[changed since publish]"));
}

#[test]
fn unpublish_removes_page_and_redeploys() {
    let fx = Fixture::new();
    fx.cmd(&["publish", "Day1/post.md"]).assert().success();
    fx.cmd(&["unpublish", "Day1/post.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unpublished Day1/post.md"));

    assert!(!fx.dir.path().join("blog/content/blog/integration-post").exists());
    // build+deploy ran twice: once for publish, once for unpublish
    assert_eq!(fx.log(&fx.rsync_log).lines().count(), 2);
    fx.cmd(&["status"])
        .assert()
        .stdout(predicate::str::contains("No published articles"));
}

#[test]
fn publish_with_local_image_colocates_it() {
    let fx = Fixture::new();
    fs::write(fx.dir.path().join("Day1/manuscript.png"), b"png-bytes").unwrap();
    fs::write(
        fx.dir.path().join("Day1/post.md"),
        "---\ntitle: Integration Post\n---\nLook:\n\n![manuscript](manuscript.png)\n",
    )
    .unwrap();
    fx.cmd(&["publish", "Day1/post.md"]).assert().success();

    let page_dir = fx.dir.path().join("blog/content/blog/integration-post");
    assert!(page_dir.join("manuscript.png").exists());
    assert!(fs::read_to_string(page_dir.join("index.md"))
        .unwrap()
        .contains("![manuscript](manuscript.png)"));
}

#[test]
fn article_without_title_fails_cleanly() {
    let fx = Fixture::new();
    fs::write(fx.dir.path().join("Day1/untitled.md"), "no heading here\n").unwrap();
    fx.cmd(&["publish", "Day1/untitled.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no title"));
}

#[test]
fn missing_config_suggests_init() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "# T\nx\n").unwrap();
    Command::cargo_bin("mdpub")
        .unwrap()
        .current_dir(dir.path())
        .args(["publish", "a.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mdpub init"));
}

#[test]
fn init_scaffolds_config() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("mdpub")
        .unwrap()
        .current_dir(dir.path())
        .args([
            "init",
            "--server",
            "deploy@203.0.113.7",
            "--base-url",
            "https://blog.example.com",
        ])
        .assert()
        .success();
    let config = fs::read_to_string(dir.path().join("mdpub.toml")).unwrap();
    assert!(config.contains("docroot = \"/var/www/blog\""));
}
