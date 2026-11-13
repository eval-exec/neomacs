//! Observe deliberate shutdown from outside the editor process. No Lisp form
//! after kill-emacs can observe unlock/unlink or final process cleanup.
#![cfg(unix)]

use crate::common::{self, OracleMode, oracle_sandbox::OracleSandbox};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
enum Probe {
    Unlock,
    AutoSaveList,
    Sentinels,
    HookOrder,
    HookError,
    #[cfg(target_os = "linux")]
    ChildHangup,
}

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    exit: Option<i32>,
    artifact_before: Option<bool>,
    artifact_after: Option<bool>,
    events: Vec<String>,
}

impl Probe {
    fn script(self) -> &'static str {
        match self {
            Self::Unlock => include_str!("fixtures/kill_emacs_shutdown/lock-release.el"),
            Self::AutoSaveList => {
                include_str!("fixtures/kill_emacs_shutdown/auto-save-list-unlink.el")
            }
            Self::Sentinels => include_str!("fixtures/kill_emacs_shutdown/sentinels.el"),
            Self::HookOrder => include_str!("fixtures/kill_emacs_shutdown/order.el"),
            Self::HookError => include_str!("fixtures/kill_emacs_shutdown/order-error.el"),
            #[cfg(target_os = "linux")]
            Self::ChildHangup => include_str!("fixtures/kill_emacs_shutdown/child-hangup.el"),
        }
    }

    fn checkpoint(self) -> bool {
        matches!(self, Self::Unlock | Self::AutoSaveList)
    }

    fn expected(self) -> Observation {
        let events: &[&str] = match self {
            Self::Unlock | Self::AutoSaveList => &[],
            Self::Sentinels => &[
                "sentinel:control",
                "kill-hook",
                "sentinel:hook-control",
                "shutdown-save",
                "created-live",
                "after-delete",
            ],
            Self::HookOrder => &[
                "auto-save-hook",
                "save:first",
                "kill-hook",
                "save:first+second",
            ],
            Self::HookError => &[
                "auto-save-hook",
                "save:first",
                "kill-hook-error",
                "kill-hook-next",
                "save:first+second",
            ],
            #[cfg(target_os = "linux")]
            Self::ChildHangup => &[
                "READY",
                "AUTOSAVE-1=(t nil)",
                "BEFORE-KILL",
                "KILL-HOOK",
                "AUTOSAVE-2=(t t)",
            ],
        };
        Observation {
            exit: Some(0),
            artifact_before: self.checkpoint().then_some(true),
            artifact_after: self.checkpoint().then_some(false),
            events: events.iter().map(|s| (*s).into()).collect(),
        }
    }
}

struct RunningEditor {
    child: Child,
    root: PathBuf,
}

impl RunningEditor {
    fn diagnostics(&self) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            fs::read_to_string(self.root.join("stdout")).unwrap_or_default(),
            fs::read_to_string(self.root.join("stderr")).unwrap_or_default(),
        )
    }

    fn checkpoint(&mut self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !path.exists() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "editor exited before the native-artifact checkpoint: {}",
                self.diagnostics()
            );
            assert!(
                Instant::now() < deadline,
                "checkpoint timeout: {}",
                self.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "shutdown timeout: {}",
                self.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for RunningEditor {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // The bounded helper watches this editor's lifetime and exits when
        // its parent changes, including when this guard kills the editor.
    }
}

fn observe(probe: Probe, binary: &Path) -> Observation {
    let case = OracleSandbox::create_fixture_tempdir().unwrap();
    let root = case.path();
    let target = root.join("visited.txt");
    let artifact = match probe {
        Probe::Unlock => root.join(".#visited.txt"),
        _ => root.join("auto-save-list"),
    };
    fs::write(&target, "initial saved content\n").unwrap();
    if matches!(probe, Probe::AutoSaveList) {
        fs::write(&artifact, "preexisting auto-save session list\n").unwrap();
    }
    let script = root.join("probe.el");
    fs::write(&script, probe.script()).unwrap();
    #[cfg(target_os = "linux")]
    if matches!(probe, Probe::ChildHangup) {
        fs::write(
            root.join("child.py"),
            include_str!("fixtures/kill_emacs_shutdown/child-hangup.py"),
        )
        .unwrap();
    }
    let sandbox = OracleSandbox::new(
        probe.script(),
        &[],
        &common::oracle_sandbox::project_root().join("lisp"),
    )
    .unwrap();
    let mut command = sandbox.pinned_command(binary);
    sandbox.configure(&mut command);
    command
        .args(["--batch", "--quick", "--load"])
        .arg(&script)
        .env("SHUTDOWN_TARGET", &target)
        .env("SHUTDOWN_ARTIFACT", &artifact)
        .env("SHUTDOWN_READY", root.join("ready"))
        .env("SHUTDOWN_GO", root.join("go"))
        .env("SHUTDOWN_LOG", root.join("events.log"))
        .env("SHUTDOWN_AUTOSAVE", root.join("autosave-handler"))
        .env("SHUTDOWN_PROBE_ROOT", root)
        .env("SHUTDOWN_CHILD_PROGRAM", "python3")
        .env("SHUTDOWN_CHILD_HELPER", root.join("child.py"))
        .stdin(Stdio::null())
        .stdout(fs::File::create(root.join("stdout")).unwrap())
        .stderr(fs::File::create(root.join("stderr")).unwrap());
    let mut editor = RunningEditor {
        child: command.spawn().unwrap(),
        root: root.into(),
    };
    let before = if probe.checkpoint() {
        editor.checkpoint(&root.join("ready"));
        let exists = fs::symlink_metadata(&artifact).is_ok();
        assert!(
            exists,
            "native artifact absent before kill: {}",
            editor.diagnostics()
        );
        if matches!(probe, Probe::Unlock) {
            assert!(
                fs::symlink_metadata(&artifact)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
        fs::write(root.join("go"), "go\n").unwrap();
        Some(exists)
    } else {
        None
    };
    let status = editor.wait();
    let events = match fs::read_to_string(root.join("events.log")) {
        Ok(text) => text.lines().map(str::to_owned).collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(error) => panic!("read shutdown observations: {error}"),
    };
    let result = Observation {
        exit: status.code(),
        artifact_before: before,
        artifact_after: probe
            .checkpoint()
            .then(|| fs::symlink_metadata(&artifact).is_ok()),
        events,
    };
    eprintln!(
        "{probe:?} {} -> {result:?}\n{}",
        binary.display(),
        editor.diagnostics()
    );
    result
}

fn assert_shutdown_parity(probe: Probe) {
    let neo = || observe(probe, Path::new(&common::neomacs_binary_path()));
    let expected = probe.expected();
    match OracleMode::from_env() {
        OracleMode::Snapshot => assert_eq!(neo(), expected),
        OracleMode::Verify | OracleMode::Live => {
            let gnu = observe(probe, Path::new(&common::oracle_emacs_path()));
            assert_eq!(gnu, expected, "GNU prerequisites/contract for {probe:?}");
            assert_eq!(neo(), gnu, "shutdown parity for {probe:?}");
        }
        OracleMode::Refresh => {
            panic!("shutdown observers have no refreshable snapshots; use verify")
        }
    }
}

#[test]
fn shutdown_unlocks_live_modified_buffers_after_kill_emacs() {
    assert_shutdown_parity(Probe::Unlock);
}

#[test]
fn shutdown_unlinks_the_auto_save_list_after_deliberate_exit() {
    assert_shutdown_parity(Probe::AutoSaveList);
}

#[test]
fn shutdown_suppresses_sentinels_even_for_processes_created_by_auto_save() {
    assert_shutdown_parity(Probe::Sentinels);
}

#[test]
fn shutdown_runs_kill_hook_before_the_canonical_second_auto_save() {
    assert_shutdown_parity(Probe::HookOrder);
}

#[test]
fn shutdown_batch_hook_error_does_not_skip_the_next_hook() {
    assert_shutdown_parity(Probe::HookError);
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_sends_child_sighup_before_the_second_auto_save() {
    assert_shutdown_parity(Probe::ChildHangup);
}
