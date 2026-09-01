use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Structured process description for a TUI session.
///
/// Arguments and environment values remain distinct OS strings, so package
/// startup files and paths containing whitespace never pass through shell-like
/// tokenization.
#[derive(Clone, Debug)]
pub struct TuiLaunch {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
    pub(crate) env: Vec<(OsString, OsString)>,
    pub(crate) env_remove: Vec<OsString>,
    pub(crate) current_dir: Option<PathBuf>,
}

impl TuiLaunch {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            env_remove: Vec::new(),
            current_dir: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    pub fn envs<I, K, V>(mut self, environment: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.env.extend(
            environment
                .into_iter()
                .map(|(name, value)| (name.into(), value.into())),
        );
        self
    }

    pub fn env_remove(mut self, name: impl Into<OsString>) -> Self {
        self.env_remove.push(name.into());
        self
    }

    pub fn current_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(directory.into());
        self
    }

    pub(crate) fn environment_value(&self, name: &str) -> Option<&OsStr> {
        self.env
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_os_str())
    }
}

impl From<&str> for TuiLaunch {
    fn from(command_line: &str) -> Self {
        let mut parts = command_line.split_whitespace();
        let program = parts
            .next()
            .expect("TUI command line must contain an executable");
        Self::new(program).args(parts)
    }
}

impl From<&Path> for TuiLaunch {
    fn from(program: &Path) -> Self {
        Self::new(program.as_os_str())
    }
}
