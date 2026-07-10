use anyhow::Result;

use crate::config::Workspace;
use crate::runner::Runner;

/// `zola build --base-url <public url>` in the site directory.
pub fn build(runner: &mut dyn Runner, ws: &Workspace) -> Result<()> {
    runner.run(
        &ws.config.zola_bin,
        &["build".into(), "--base-url".into(), ws.config.base_url.clone()],
        Some(&ws.site_dir()),
    )
}

/// rsync the built site to the server docroot. `--delete` keeps the
/// remote an exact mirror, so unpublished pages disappear.
pub fn deploy(runner: &mut dyn Runner, ws: &Workspace) -> Result<()> {
    let public = ws.site_dir().join("public");
    runner.run(
        &ws.config.rsync_bin,
        &[
            "-az".into(),
            "--delete".into(),
            format!("{}/", public.display()),
            format!("{}:{}/", ws.config.server, ws.config.docroot.trim_end_matches('/')),
        ],
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::Config;
    use crate::runner::MockRunner;

    fn workspace() -> Workspace {
        Workspace {
            config: Config {
                base_url: "https://blog.example.com".into(),
                server: "deploy@203.0.113.7".into(),
                docroot: "/var/www/blog/".into(),
                site_dir: PathBuf::from("blog"),
                zola_bin: "zola".into(),
                rsync_bin: "rsync".into(),
            },
            root: PathBuf::from("/repo"),
        }
    }

    #[test]
    fn build_runs_zola_in_site_dir_with_base_url() {
        let mut mock = MockRunner::default();
        build(&mut mock, &workspace()).unwrap();
        assert_eq!(mock.calls.len(), 1);
        let call = &mock.calls[0];
        assert_eq!(call.program, "zola");
        assert_eq!(call.args, vec!["build", "--base-url", "https://blog.example.com"]);
        assert_eq!(call.cwd.as_deref(), Some(std::path::Path::new("/repo/blog")));
    }

    #[test]
    fn deploy_rsyncs_public_to_docroot() {
        let mut mock = MockRunner::default();
        deploy(&mut mock, &workspace()).unwrap();
        assert_eq!(mock.calls.len(), 1);
        let call = &mock.calls[0];
        assert_eq!(call.program, "rsync");
        assert_eq!(
            call.args,
            vec![
                "-az",
                "--delete",
                "/repo/blog/public/",
                "deploy@203.0.113.7:/var/www/blog/",
            ]
        );
        assert_eq!(call.cwd, None);
    }
}
