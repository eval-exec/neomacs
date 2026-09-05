use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use neomacs_tui_tests::{
    QUIET_NATIVE_COMP_EVAL, RawTerminalSnapshot, TuiLaunch, TuiRecordingScope, TuiSession,
    assert_raw_terminal_snapshots_eq, compare_session_displays,
};

use crate::{EmacsRuntime, MelpaSandbox, PreparedPackageSet};

/// Terminal color capability shared by both editors in a package parity pair.
///
/// Keeping the supported profiles closed prevents a test from accidentally
/// changing GNU and Neomacs through unrelated environment overrides.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalProfile {
    Indexed256,
    #[default]
    TrueColor,
}

/// Observable terminal-state contract for a package display checkpoint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PackageDisplayContract {
    /// Compare visible text, geometry, cursor, faces, and resolved RGB colors.
    #[default]
    ExactDisplay,
    /// Compare the terminal's raw cell state, including indexed color values.
    RawTerminal,
}

/// Startup deadlines for a symmetric package TUI pair.
///
/// Most scenarios should use [`Self::same`]. A slower Neomacs startup must be
/// declared explicitly at the call site instead of being hidden in the
/// harness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairTimeout {
    Same(Duration),
    PerEditor { gnu: Duration, neomacs: Duration },
}

impl PairTimeout {
    pub const fn same(timeout: Duration) -> Self {
        Self::Same(timeout)
    }

    pub const fn per_editor(gnu: Duration, neomacs: Duration) -> Self {
        Self::PerEditor { gnu, neomacs }
    }

    const fn split(self) -> (Duration, Duration) {
        match self {
            Self::Same(timeout) => (timeout, timeout),
            Self::PerEditor { gnu, neomacs } => (gnu, neomacs),
        }
    }
}

/// The startup observation that makes a spawned pair safe to drive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadinessCheckpoint {
    description: String,
    timeout: PairTimeout,
}

impl ReadinessCheckpoint {
    pub fn new(description: impl Into<String>, timeout: PairTimeout) -> Self {
        Self {
            description: description.into(),
            timeout,
        }
    }
}

/// A complete-display assertion selected deliberately by a package scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayCheckpoint {
    label: String,
    contract: PackageDisplayContract,
}

impl DisplayCheckpoint {
    /// Create the ordinary complete-screen contract: exact visible display
    /// parity after terminal colors have been resolved to RGB.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            contract: PackageDisplayContract::ExactDisplay,
        }
    }

    pub fn raw_terminal(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            contract: PackageDisplayContract::RawTerminal,
        }
    }
}

/// Builder for one package fixture launched against GNU Emacs and Neomacs.
pub struct PackageTuiScenario {
    label: String,
    packages: PreparedPackageSet,
    terminal_profile: TerminalProfile,
}

impl PackageTuiScenario {
    pub fn new(label: impl Into<String>, packages: &PreparedPackageSet) -> Self {
        Self {
            label: label.into(),
            packages: packages.clone(),
            terminal_profile: TerminalProfile::default(),
        }
    }

    #[must_use]
    pub fn terminal_profile(mut self, terminal_profile: TerminalProfile) -> Self {
        self.terminal_profile = terminal_profile;
        self
    }

    /// Spawn both peers, but expose no sessions until readiness is observed.
    fn spawn(self) -> Result<StartingPackageTuiPair, String> {
        StartingPackageTuiPair::spawn(&self.label, &self.packages, self.terminal_profile)
    }

    pub fn spawn_when_ready<F>(
        self,
        checkpoint: ReadinessCheckpoint,
        predicate: F,
    ) -> Result<PackageTuiPair, String>
    where
        F: Fn(&[String]) -> bool,
    {
        self.spawn()?.wait_until_ready(checkpoint, predicate)
    }
}

/// A pair whose processes exist but have not both reached their first screen.
///
/// Its sessions are deliberately private. Consuming
/// [`Self::wait_until_ready`] is the only conversion to a drivable pair.
struct StartingPackageTuiPair {
    label: String,
    gnu: TuiSession,
    neo: TuiSession,
    gnu_sandbox: MelpaSandbox,
    neo_sandbox: MelpaSandbox,
}

pub struct PackageTuiPair {
    /// Direct oracle access for package-specific asynchronous observation.
    /// Prefer the symmetric helpers for ordinary actions.
    pub gnu: TuiSession,
    /// Direct implementation access for package-specific asynchronous
    /// observation. Prefer the symmetric helpers for ordinary actions.
    pub neo: TuiSession,
    label: String,
    _gnu_sandbox: MelpaSandbox,
    _neo_sandbox: MelpaSandbox,
}

impl StartingPackageTuiPair {
    fn spawn(
        label: &str,
        packages: &PreparedPackageSet,
        terminal_profile: TerminalProfile,
    ) -> Result<Self, String> {
        let display_env = SymmetricDisplayEnvironment::from(terminal_profile);
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

        let recording_scope = TuiRecordingScope::new("neomacs-melpa-tests", label);
        Ok(Self {
            label: label.to_owned(),
            gnu: TuiSession::spawn_launch_in_scope(gnu_launch, "GNU", recording_scope.clone()),
            neo: TuiSession::spawn_launch_in_scope(neo_launch, "NEO", recording_scope),
            gnu_sandbox,
            neo_sandbox,
        })
    }

    pub fn wait_until_ready<F>(
        mut self,
        checkpoint: ReadinessCheckpoint,
        predicate: F,
    ) -> Result<PackageTuiPair, String>
    where
        F: Fn(&[String]) -> bool,
    {
        let (gnu_timeout, neomacs_timeout) = checkpoint.timeout.split();
        self.gnu.read_until(gnu_timeout, &predicate);
        self.neo.read_until(neomacs_timeout, &predicate);

        let gnu_grid = self.gnu.text_grid();
        let neo_grid = self.neo.text_grid();
        let mut failures = Vec::new();
        if !predicate(&gnu_grid) {
            failures.push(format!("GNU screen:\n{}", gnu_grid.join("\n")));
        }
        if !predicate(&neo_grid) {
            failures.push(format!("Neomacs screen:\n{}", neo_grid.join("\n")));
        }
        if !failures.is_empty() {
            return Err(format!(
                "package TUI scenario {:?} timed out waiting for {}:\n{}",
                self.label,
                checkpoint.description,
                failures.join("\n\n")
            ));
        }

        let marker = format!("ready: {}", checkpoint.description);
        self.gnu.mark_recording(&marker);
        self.neo.mark_recording(&marker);

        Ok(PackageTuiPair {
            label: self.label,
            gnu: self.gnu,
            neo: self.neo,
            _gnu_sandbox: self.gnu_sandbox,
            _neo_sandbox: self.neo_sandbox,
        })
    }
}

impl PackageTuiPair {
    /// Apply the same operation to GNU Emacs and Neomacs, in that order.
    pub fn drive_both(&mut self, mut operation: impl FnMut(&mut TuiSession)) {
        operation(&mut self.gnu);
        operation(&mut self.neo);
    }

    pub fn resize_both(&mut self, rows: u16, columns: u16) {
        self.drive_both(|session| session.resize(rows, columns));
    }

    pub fn send_both(&mut self, input: &[u8]) {
        self.drive_both(|session| session.send(input));
    }

    pub fn send_key_both(&mut self, key: &str) {
        self.drive_both(|session| session.send_key(key));
    }

    pub fn send_keys_both(&mut self, keys: &str) {
        self.drive_both(|session| session.send_keys(keys));
    }

    pub fn settle_both(&mut self, timeout: Duration) {
        self.drive_both(|session| session.read(timeout));
    }

    pub fn assert_display(&mut self, checkpoint: DisplayCheckpoint) {
        let label = format!("{}: {}", self.label, checkpoint.label);
        let marker = format!("display: {}", checkpoint.label);
        self.gnu.mark_recording(&marker);
        self.neo.mark_recording(&marker);
        match checkpoint.contract {
            PackageDisplayContract::ExactDisplay => {
                let report = compare_session_displays(&self.gnu, &self.neo);
                assert!(
                    report.is_satisfied(),
                    "{label} violated exact display parity:\n{:#?}",
                    report.unexpected()
                );
            }
            PackageDisplayContract::RawTerminal => {
                let gnu = RawTerminalSnapshot::capture_full_screen(self.gnu.screen());
                let neomacs = RawTerminalSnapshot::capture_full_screen(self.neo.screen());
                assert_raw_terminal_snapshots_eq(&label, &gnu, &neomacs);
            }
        }
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

impl From<TerminalProfile> for SymmetricDisplayEnvironment {
    fn from(profile: TerminalProfile) -> Self {
        match profile {
            TerminalProfile::Indexed256 => Self {
                set: Vec::new(),
                remove: vec![OsString::from("COLORTERM")],
            },
            TerminalProfile::TrueColor => Self {
                set: vec![(OsString::from("COLORTERM"), OsString::from("truecolor"))],
                remove: Vec::new(),
            },
        }
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
        launch = launch.arg("-no-comp-spawn").arg(QUIET_NATIVE_COMP_EVAL);
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
        DisplayCheckpoint, PackageDisplayContract, PairTimeout, SymmetricDisplayEnvironment,
        TerminalProfile, canonical_executable_identity, validate_distinct_editor_identities,
    };

    #[test]
    fn terminal_profile_owns_the_symmetric_display_environment() {
        let display = SymmetricDisplayEnvironment::from(TerminalProfile::TrueColor);
        assert_eq!(
            display
                .set_entries()
                .map(|(key, value)| (key.to_str(), value.to_str()))
                .collect::<Vec<_>>(),
            vec![(Some("COLORTERM"), Some("truecolor"))]
        );
        assert_eq!(display.removed_entries().count(), 0);

        let removed = SymmetricDisplayEnvironment::from(TerminalProfile::Indexed256);
        assert_eq!(removed.set_entries().count(), 0);
        assert_eq!(
            removed
                .removed_entries()
                .map(OsStr::to_str)
                .collect::<Vec<_>>(),
            vec![Some("COLORTERM")]
        );
        assert_eq!(TerminalProfile::default(), TerminalProfile::TrueColor);
    }

    #[test]
    fn package_display_contract_defaults_to_resolved_rgb_exact_display() {
        assert_eq!(
            PackageDisplayContract::default(),
            PackageDisplayContract::ExactDisplay
        );
        assert_eq!(
            DisplayCheckpoint::new("visible screen").contract,
            PackageDisplayContract::ExactDisplay
        );
        assert_eq!(
            DisplayCheckpoint::raw_terminal("wire state").contract,
            PackageDisplayContract::RawTerminal
        );
    }

    #[test]
    fn pair_timeout_requires_asymmetry_to_be_explicit() {
        let same = std::time::Duration::from_secs(8);
        assert_eq!(PairTimeout::same(same).split(), (same, same));

        let gnu = std::time::Duration::from_secs(6);
        let neomacs = std::time::Duration::from_secs(12);
        assert_eq!(
            PairTimeout::per_editor(gnu, neomacs).split(),
            (gnu, neomacs)
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
