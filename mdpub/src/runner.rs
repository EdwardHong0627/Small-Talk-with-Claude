use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Abstraction over external commands (zola, rsync) so the publish
/// pipeline is testable without touching the network or a real site.
pub trait Runner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<()>;
}

pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<()> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let status = cmd
            .status()
            .with_context(|| format!("running `{program}` — is it installed and on PATH?"))?;
        if !status.success() {
            bail!("`{program} {}` failed with {status}", args.join(" "));
        }
        Ok(())
    }
}

/// Records calls instead of executing them; optionally fails the nth call.
#[derive(Default)]
pub struct MockRunner {
    pub calls: Vec<RecordedCall>,
    pub fail_on: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl Runner for MockRunner {
    fn run(&mut self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<()> {
        let index = self.calls.len();
        self.calls.push(RecordedCall {
            program: program.to_string(),
            args: args.to_vec(),
            cwd: cwd.map(Path::to_path_buf),
        });
        if self.fail_on == Some(index) {
            bail!("mock failure on call {index}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_runner_succeeds_on_true() {
        let mut runner = RealRunner;
        runner.run("true", &[], None).unwrap();
    }

    #[test]
    fn real_runner_reports_failure() {
        let mut runner = RealRunner;
        let err = runner.run("false", &[], None).unwrap_err().to_string();
        assert!(err.contains("failed"), "unexpected error: {err}");
    }

    #[test]
    fn real_runner_reports_missing_binary() {
        let mut runner = RealRunner;
        let err = runner
            .run("definitely-not-a-real-binary-xyz", &[], None)
            .unwrap_err();
        assert!(format!("{err:#}").contains("on PATH"));
    }

    #[test]
    fn mock_runner_records_and_fails_on_demand() {
        let mut mock = MockRunner {
            fail_on: Some(1),
            ..MockRunner::default()
        };
        mock.run("zola", &["build".into()], None).unwrap();
        assert!(mock.run("rsync", &[], None).is_err());
        assert_eq!(mock.calls.len(), 2);
        assert_eq!(mock.calls[0].program, "zola");
        assert_eq!(mock.calls[0].args, vec!["build"]);
    }
}
