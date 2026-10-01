//! Minimal fixtures for the moved source-lock tests.
//!
//! The source-lock contract tests exercise git plumbing and cache locking
//! against a fake "editor" script; they never needed the real attested
//! runtime, so this module provides the smallest driver that satisfies
//! [`PackageInstallDriver`].

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use super::install::PackageInstallDriver;

pub(crate) struct TestSandbox(tempfile::TempDir);

impl TestSandbox {
    pub(crate) fn new(label: &str) -> Result<Self, String> {
        let directory = tempfile::Builder::new()
            .prefix(&format!("infra-packages-{label}-"))
            .tempdir()
            .map_err(|error| format!("failed to create test sandbox: {error}"))?;
        Ok(Self(directory))
    }

    pub(crate) fn root(&self) -> PathBuf {
        self.0.path().to_path_buf()
    }
}

pub(crate) struct TestRuntime {
    name: &'static str,
    script: PathBuf,
    envs: Vec<(String, String)>,
    timeout: Duration,
}

impl TestRuntime {
    pub(crate) fn new(name: &'static str, script: impl Into<PathBuf>) -> Self {
        Self {
            name,
            script: script.into(),
            envs: Vec::new(),
            timeout: Duration::from_secs(60),
        }
    }

    pub(crate) fn with_env(mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        self.envs.push((
            key.to_string(),
            value.as_ref().to_string_lossy().into_owned(),
        ));
        self
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl PackageInstallDriver for TestRuntime {
    fn name(&self) -> &str {
        self.name
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.script);
        for (key, value) in &self.envs {
            command.env(key, value);
        }
        command
    }

    fn run(
        &self,
        command: &mut Command,
    ) -> Result<std::process::Output, super::install::InstallCommandError> {
        use wait_timeout::ChildExt;
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(super::install::InstallCommandError::Launch)?;
        match child.wait_timeout(self.timeout) {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(super::install::InstallCommandError::TimedOut(
                    std::process::Output {
                        status: child
                            .wait()
                            .map_err(super::install::InstallCommandError::Launch)?,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    },
                ));
            }
            Err(error) => return Err(super::install::InstallCommandError::Launch(error)),
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut pipe) = child.stdout.take() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut stderr);
        }
        Ok(std::process::Output {
            status: child
                .wait()
                .map_err(super::install::InstallCommandError::Launch)?,
            stdout,
            stderr,
        })
    }
}
