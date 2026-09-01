use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use neomacs_tui_tests::{TuiLaunch, TuiSession};

use crate::{EmacsRuntime, MelpaSandbox, PreparedPackageSet};

const QUIET_GNU_NATIVE_COMP: &str = "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent)(push'(native-compiler)warning-suppress-types)(mapc'kill-process(process-list)))";

pub struct PackageTuiPair {
    pub gnu: TuiSession,
    pub neo: TuiSession,
    _gnu_sandbox: MelpaSandbox,
    _neo_sandbox: MelpaSandbox,
}

/// A symmetric terminal-environment operation for a real package TUI pair.
///
/// The key remains explicit for validation and diagnostics, while the enum
/// makes inherited-value removal impossible to conflate with setting an empty
/// value.  The package adapter deliberately accepts only `COLORTERM`; `TERM`,
/// sandbox, executable, and package environment ownership stay with the shared
/// harness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayEnvOverride<'a> {
    Set { key: &'a str, value: &'a str },
    Remove { key: &'a str },
}

impl PackageTuiPair {
    pub fn spawn(label: &str, packages: &PreparedPackageSet) -> Result<Self, String> {
        Self::spawn_with_display_env(label, packages, &[])
    }

    pub fn spawn_with_display_env(
        label: &str,
        packages: &PreparedPackageSet,
        display_env: &[DisplayEnvOverride<'_>],
    ) -> Result<Self, String> {
        let display_env = SymmetricDisplayEnvironment::try_from(display_env)?;
        let gnu_runtime = EmacsRuntime::gnu_emacs();
        let neo_runtime = EmacsRuntime::neomacs();
        let gnu_identity = canonical_executable_identity(&gnu_runtime.executable)?;
        let neo_identity = canonical_executable_identity(&neo_runtime.executable)?;
        validate_distinct_editor_identities(
            &gnu_identity,
            &neo_identity,
            env::var_os("UPDATE_EXPECT").as_deref() == Some(OsStr::new("1")),
        )?;
        let gnu_sandbox = MelpaSandbox::new(&format!("{label}-tui-gnu"))?;
        let neo_sandbox = MelpaSandbox::new(&format!("{label}-tui-neo"))?;
        let gnu_startup_file = packages.write_startup_file(gnu_sandbox.root())?;
        let neo_startup_file = packages.write_startup_file(neo_sandbox.root())?;

        let gnu_launch = editor_launch(
            gnu_runtime,
            &gnu_sandbox,
            packages,
            &gnu_startup_file,
            &display_env,
            true,
        );
        let neo_launch = editor_launch(
            neo_runtime,
            &neo_sandbox,
            packages,
            &neo_startup_file,
            &display_env,
            false,
        );

        Ok(Self {
            gnu: TuiSession::spawn_launch(gnu_launch, "GNU"),
            neo: TuiSession::spawn_launch(neo_launch, "NEO"),
            _gnu_sandbox: gnu_sandbox,
            _neo_sandbox: neo_sandbox,
        })
    }
}

fn canonical_executable_identity(executable: &Path) -> Result<PathBuf, String> {
    let resolved = if executable.components().count() > 1 || executable.is_absolute() {
        executable.to_path_buf()
    } else {
        let path = env::var_os("PATH").ok_or_else(|| {
            format!("package TUI cannot resolve executable {executable:?}: PATH is absent")
        })?;
        env::split_paths(&path)
            .map(|directory| directory.join(executable))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                format!("package TUI cannot resolve executable {executable:?} through PATH")
            })?
    };
    fs::canonicalize(&resolved).map_err(|error| {
        format!(
            "package TUI cannot canonicalize editor executable {}: {error}",
            resolved.display()
        )
    })
}

fn validate_distinct_editor_identities(
    gnu: &Path,
    neo: &Path,
    allow_equal_for_expect_update: bool,
) -> Result<(), String> {
    if gnu == neo && !allow_equal_for_expect_update {
        return Err(format!(
            "package TUI GNU and Neo executables resolve to the same binary {}; set UPDATE_EXPECT=1 only for deliberate GNU snapshot calibration",
            gnu.display()
        ));
    }
    Ok(())
}

/// A validated display environment applied identically to both real editors.
///
/// The PTY owner already fixes `TERM`; package tests may only make `COLORTERM`
/// explicit or deliberately remove its inherited value.  One owned plan is
/// shared by both launch builders so the type prevents accidentally configuring
/// just one peer.
#[derive(Debug, Default, Eq, PartialEq)]
struct SymmetricDisplayEnvironment {
    set: Vec<(OsString, OsString)>,
    remove: Vec<OsString>,
}

impl TryFrom<&[DisplayEnvOverride<'_>]> for SymmetricDisplayEnvironment {
    type Error = String;

    fn try_from(entries: &[DisplayEnvOverride<'_>]) -> Result<Self, Self::Error> {
        let mut set = Vec::with_capacity(entries.len());
        let mut remove = Vec::with_capacity(entries.len());
        let mut seen: Vec<&str> = Vec::with_capacity(entries.len());
        for entry in entries {
            let key = *match entry {
                DisplayEnvOverride::Set { key, .. } | DisplayEnvOverride::Remove { key } => key,
            };
            if key.is_empty() || key.contains('\0') || key.contains('=') {
                return Err(format!(
                    "package TUI display environment has invalid key {key:?}"
                ));
            }
            if key != "COLORTERM" {
                return Err(format!(
                    "package TUI display environment may not override reserved key {key:?}"
                ));
            }
            if seen.contains(&key) {
                return Err(format!(
                    "package TUI display environment repeats or contradicts key {key:?}"
                ));
            }
            seen.push(key);
            match entry {
                DisplayEnvOverride::Set { value, .. } => {
                    if value.contains('\0') {
                        return Err(format!(
                            "package TUI display environment has invalid value for {key:?}"
                        ));
                    }
                    if *value != "truecolor" {
                        return Err(format!(
                            "package TUI COLORTERM must be exactly \"truecolor\", got {value:?}"
                        ));
                    }
                    set.push((OsString::from(key), OsString::from(value)));
                }
                DisplayEnvOverride::Remove { .. } => remove.push(OsString::from(key)),
            }
        }
        Ok(Self { set, remove })
    }
}

impl SymmetricDisplayEnvironment {
    fn set_entries(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.set
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str()))
    }

    fn removed_entries(&self) -> impl Iterator<Item = &OsStr> {
        self.remove.iter().map(OsString::as_os_str)
    }
}

fn editor_launch(
    runtime: EmacsRuntime,
    sandbox: &MelpaSandbox,
    packages: &PreparedPackageSet,
    startup_file: &Path,
    display_env: &SymmetricDisplayEnvironment,
    gnu: bool,
) -> TuiLaunch {
    let mut launch = TuiLaunch::new(runtime.executable.as_os_str()).args(["-nw", "-Q"]);
    if gnu {
        launch = launch.arg("-no-comp-spawn").arg(QUIET_GNU_NATIVE_COMP);
    }
    let mut launch = launch
        .arg("--load")
        .arg(startup_file.as_os_str())
        .envs(sandbox.process_environment())
        .envs(packages.process_environment())
        .envs(display_env.set_entries())
        .env_remove("EMACSLOADPATH")
        .env("TERM", "screen-256color")
        .current_dir(sandbox.root());
    for key in display_env.removed_entries() {
        launch = launch.env_remove(key);
    }
    launch
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    #[cfg(unix)]
    use crate::MelpaSandbox;

    use super::{
        DisplayEnvOverride, SymmetricDisplayEnvironment, canonical_executable_identity,
        validate_distinct_editor_identities,
    };

    #[test]
    fn display_environment_is_narrow_validated_and_owned_once_for_both_peers() {
        let display = SymmetricDisplayEnvironment::try_from(
            &[DisplayEnvOverride::Set {
                key: "COLORTERM",
                value: "truecolor",
            }][..],
        )
        .expect("accept the real truecolor terminal convention");
        assert_eq!(
            display
                .set_entries()
                .map(|(key, value)| (key.to_str(), value.to_str()))
                .collect::<Vec<_>>(),
            vec![(Some("COLORTERM"), Some("truecolor"))]
        );
        assert_eq!(display.removed_entries().count(), 0);

        let removed = SymmetricDisplayEnvironment::try_from(
            &[DisplayEnvOverride::Remove { key: "COLORTERM" }][..],
        )
        .expect("accept removal for the real 256-color profile");
        assert_eq!(removed.set_entries().count(), 0);
        assert_eq!(
            removed
                .removed_entries()
                .map(OsStr::to_str)
                .collect::<Vec<_>>(),
            vec![Some("COLORTERM")]
        );

        assert!(
            SymmetricDisplayEnvironment::try_from(
                &[DisplayEnvOverride::Set {
                    key: "TERM",
                    value: "xterm-256color"
                }][..]
            )
            .is_err()
        );
        assert!(
            SymmetricDisplayEnvironment::try_from(
                &[DisplayEnvOverride::Remove { key: "HOME" }][..]
            )
            .is_err()
        );
        assert!(
            SymmetricDisplayEnvironment::try_from(
                &[DisplayEnvOverride::Set {
                    key: "COLORTERM",
                    value: "24bit"
                }][..]
            )
            .is_err()
        );
        assert!(
            SymmetricDisplayEnvironment::try_from(
                &[DisplayEnvOverride::Set {
                    key: "COLORTERM",
                    value: "true\0color"
                }][..]
            )
            .is_err()
        );
        assert!(
            SymmetricDisplayEnvironment::try_from(
                &[
                    DisplayEnvOverride::Set {
                        key: "COLORTERM",
                        value: "truecolor"
                    },
                    DisplayEnvOverride::Remove { key: "COLORTERM" }
                ][..]
            )
            .is_err()
        );
        assert_eq!(
            SymmetricDisplayEnvironment::try_from(&[][..])
                .expect("preserve the inherited/default display environment"),
            SymmetricDisplayEnvironment::default()
        );
    }

    #[test]
    fn editor_identity_rejects_accidental_same_binary_but_allows_calibration() {
        let gnu = Path::new("/canonical/gnu-emacs");
        let neo = Path::new("/canonical/neomacs");
        assert!(validate_distinct_editor_identities(gnu, neo, false).is_ok());
        assert!(validate_distinct_editor_identities(gnu, gnu, false).is_err());
        assert!(validate_distinct_editor_identities(gnu, gnu, true).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn editor_identity_canonicalizes_symlink_aliases_before_comparison() {
        let sandbox = MelpaSandbox::new("tui-editor-identity-contract")
            .expect("create owned executable-identity sandbox below ./tmp");
        let executable = sandbox.root().join("real-editor");
        let alias = sandbox.root().join("editor-alias");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n").expect("write owned executable fixture");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("make owned fixture executable");
        symlink(&executable, &alias).expect("create owned executable symlink alias");

        let executable = canonical_executable_identity(&executable)
            .expect("canonicalize the real executable fixture");
        let alias = canonical_executable_identity(&alias)
            .expect("canonicalize the executable symlink alias");
        assert_eq!(alias, executable);
        assert!(validate_distinct_editor_identities(&executable, &alias, false).is_err());
    }
}
