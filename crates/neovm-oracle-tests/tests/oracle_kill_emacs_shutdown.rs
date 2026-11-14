//! Observe deliberate shutdown from outside the editor process. No Lisp form
//! after kill-emacs can observe unlock/unlink or final process cleanup.
#![cfg(unix)]

#[cfg(target_os = "linux")]
#[path = "shutdown_native_child.rs"]
mod shutdown_native_child;
#[cfg(target_os = "linux")]
use shutdown_native_child::{
    ChildAfterEditor, HupDisposition, NativeChild, NativeObservation,
    prepare_native_child_supervision,
};

use crate::common::{self, OracleMode, oracle_sandbox::OracleSandbox};
use neomacs_test_env::bounded_temp_dir;
use std::ffi::OsString;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::process::ExitStatusExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
enum IgnoredHupProbe {
    Pipe,
    ForegroundDaemonPipe,
    Pty,
    PtyWithStderr,
    ExplicitDeleteControl,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct NativeChildSpec {
    disposition: HupDisposition,
    after_editor: ChildAfterEditor,
    helper: &'static str,
}

#[derive(Clone, Copy, Debug)]
enum EditorMode {
    Batch,
    ForegroundDaemon,
}

#[derive(Clone, Copy, Debug)]
enum Probe {
    Unlock,
    AutoSaveList,
    ListPostlude(&'static str),
    Sentinels,
    HookOrder,
    HookError,
    SafeHookDebugger,
    SafeHookDebugOnSignal,
    UnlockMutation(&'static str),
    KillBufferUnlock(&'static str),
    PipeServerSentinels,
    DaemonUnixListener,
    #[cfg(target_os = "linux")]
    IgnoredChildHangup(IgnoredHupProbe),
    HookThrowAbort,
    HookOuterHandler,
    HookNoCatch,
    HookUnintern,
    #[cfg(target_os = "linux")]
    ChildHangup,
}

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    exit: Option<i32>,
    signal: Option<i32>,
    artifact_before: Option<bool>,
    artifact_after: Option<bool>,
    secondary_after: Option<bool>,
    events: Vec<String>,
    #[cfg(target_os = "linux")]
    native_child: Option<NativeObservation>,
}

impl Probe {
    fn editor_mode(self) -> EditorMode {
        match self {
            Self::DaemonUnixListener => EditorMode::ForegroundDaemon,
            #[cfg(target_os = "linux")]
            Self::IgnoredChildHangup(IgnoredHupProbe::ForegroundDaemonPipe) => {
                EditorMode::ForegroundDaemon
            }
            _ => EditorMode::Batch,
        }
    }

    #[cfg(target_os = "linux")]
    fn native_child_spec(self) -> Option<NativeChildSpec> {
        match self {
            Self::KillBufferUnlock("real-hup") => Some(NativeChildSpec {
                disposition: HupDisposition::Caught,
                after_editor: ChildAfterEditor::Exited,
                helper: include_str!("fixtures/kill_emacs_shutdown/kill-buffer-unlock-real-hup.py"),
            }),
            Self::IgnoredChildHangup(kind) => Some(NativeChildSpec {
                disposition: HupDisposition::Ignored,
                after_editor: match kind {
                    IgnoredHupProbe::ExplicitDeleteControl => ChildAfterEditor::Exited,
                    IgnoredHupProbe::Pipe
                    | IgnoredHupProbe::ForegroundDaemonPipe
                    | IgnoredHupProbe::Pty
                    | IgnoredHupProbe::PtyWithStderr => ChildAfterEditor::Alive,
                },
                helper: include_str!("fixtures/kill_emacs_shutdown/ignored-hup-child.py"),
            }),
            _ => None,
        }
    }

    fn script(self) -> &'static str {
        match self {
            Self::Unlock => include_str!("fixtures/kill_emacs_shutdown/lock-release.el"),
            Self::ListPostlude(_) => include_str!("fixtures/kill_emacs_shutdown/list-postlude.el"),
            Self::UnlockMutation(_) => {
                include_str!("fixtures/kill_emacs_shutdown/unlock-mutation.el")
            }
            Self::KillBufferUnlock("real-hup") => {
                include_str!("fixtures/kill_emacs_shutdown/kill-buffer-unlock-real-hup.el")
            }
            Self::KillBufferUnlock("current-indirect-frame-preference") => {
                include_str!(
                    "fixtures/kill_emacs_shutdown/kill-buffer-current-indirect-preference.el"
                )
            }
            Self::KillBufferUnlock("current-indirect-base") => {
                include_str!("fixtures/kill_emacs_shutdown/kill-buffer-current-indirect.el")
            }
            Self::KillBufferUnlock(_) => {
                include_str!("fixtures/kill_emacs_shutdown/kill-buffer-unlock.el")
            }
            Self::DaemonUnixListener => {
                include_str!("fixtures/kill_emacs_shutdown/daemon-unix-listener.el")
            }
            Self::PipeServerSentinels => {
                include_str!("fixtures/kill_emacs_shutdown/pipe-server-sentinels.el")
            }
            #[cfg(target_os = "linux")]
            Self::IgnoredChildHangup(_) => {
                include_str!("fixtures/kill_emacs_shutdown/ignored-hup-child.el")
            }
            Self::AutoSaveList => {
                include_str!("fixtures/kill_emacs_shutdown/auto-save-list-unlink.el")
            }
            Self::Sentinels => include_str!("fixtures/kill_emacs_shutdown/sentinels.el"),
            Self::HookOrder => include_str!("fixtures/kill_emacs_shutdown/order.el"),
            Self::HookError => include_str!("fixtures/kill_emacs_shutdown/order-error.el"),
            Self::SafeHookDebugger => {
                include_str!("fixtures/kill_emacs_shutdown/safe-hook-debugger.el")
            }
            Self::SafeHookDebugOnSignal => {
                include_str!("fixtures/kill_emacs_shutdown/safe-hook-debug-on-signal.el")
            }
            Self::HookThrowAbort => {
                include_str!("fixtures/kill_emacs_shutdown/hook-throw-abort.el")
            }
            Self::HookOuterHandler => {
                include_str!("fixtures/kill_emacs_shutdown/hook-outer-handler.el")
            }
            Self::HookNoCatch => include_str!("fixtures/kill_emacs_shutdown/hook-no-catch.el"),
            Self::HookUnintern => include_str!("fixtures/kill_emacs_shutdown/hook-unintern.el"),
            #[cfg(target_os = "linux")]
            Self::ChildHangup => include_str!("fixtures/kill_emacs_shutdown/child-hangup.el"),
        }
    }

    fn checkpoint(self) -> bool {
        matches!(
            self,
            Self::Unlock
                | Self::AutoSaveList
                | Self::ListPostlude(_)
                | Self::DaemonUnixListener
                | Self::UnlockMutation("unlock-throw" | "post-dying-state")
        )
    }

    fn signal(self) -> Option<i32> {
        match self {
            Self::ListPostlude("signal-term") => Some(libc::SIGTERM),
            Self::ListPostlude("signal-abort") => Some(libc::SIGABRT),
            _ => None,
        }
    }

    fn secondary_path(self, root: &Path) -> Option<PathBuf> {
        match self {
            Self::ListPostlude("unintern-nil" | "unintern-decoy") => {
                Some(root.join("replacement-list"))
            }
            Self::ListPostlude("postlude-missing") => Some(root.join("missing-list")),
            Self::ListPostlude("postlude-directory") => Some(root.join("list-directory")),
            _ => None,
        }
    }

    fn expected(self) -> Observation {
        let events: &[&str] = match self {
            Self::Unlock | Self::AutoSaveList => &[],
            Self::ListPostlude(kind) => match kind {
                "postlude-raw-bypass" => &[
                    "delete-file-handler",
                    "delete-handler-control-caught",
                    "delete-function-cell",
                    "delete-function-control-caught",
                    "ready",
                    "save-set-list",
                ],
                "postlude-expand-function-cell" => &["expand-function-control-caught", "ready"],
                "postlude-input-nul" => &[
                    "input-nul-control-filenamep",
                    "ready",
                    "save-set-list",
                    "postlude-input-nul-caught:filenamep",
                    "second-kill",
                ],
                "postlude-expand-nul" => &[
                    "expand-handler",
                    "expand-nul-control-exact=t:suffix-bytes=(0 115 117 102 102 105 120)",
                    "ready",
                    "save-set-list",
                    "expand-handler",
                ],
                "postlude-expand-handler" => &[
                    "expand-handler",
                    "expand-control-ok",
                    "ready",
                    "save-set-list",
                    "expand-handler",
                ],
                "postlude-expand-escape" => &[
                    "expand-handler",
                    "expand-control-ok",
                    "ready",
                    "save-set-list",
                    "expand-handler",
                    "escaped:expanded-escape",
                    "second-kill",
                ],
                "postlude-missing" => &[
                    "expand-handler",
                    "expand-control-ok",
                    "ready",
                    "save-set-list",
                    "expand-handler",
                ],
                "postlude-directory" => &[
                    "expand-handler",
                    "expand-control-ok",
                    "ready",
                    "save-set-list",
                    "expand-handler",
                ],
                "save-escape" => &[
                    "ready",
                    "save-escape",
                    "escaped:saved-escape",
                    "second-kill",
                ],
                "nonstr-nil" => &["ready"],
                "nonstr-t" => &["ready"],
                "nonstr-int" => &["ready"],
                "nonstr-cons" => &["ready"],
                "unintern-nil" => &[
                    "identity:distinct=t:original-preserved=t:replacement=nil",
                    "ready",
                ],
                "unintern-decoy" => &[
                    "identity:distinct=t:original-preserved=t:replacement=decoy",
                    "ready",
                ],
                "signal-term" => &["ready"],
                "signal-abort" => &["ready"],
                _ => panic!("unknown list-postlude case: {kind}"),
            },
            Self::UnlockMutation(kind) => match kind {
                "append" => &["unlock:a", "unlock:b", "unlock:c"],
                "rename-later" => &["unlock:a", "unlock:b-renamed"],
                "clear-later" => &["unlock:a"],
                "bury-current" => &["unlock:a"],
                "kill-current" => &["unlock:a", "unlock:a", "unlock:b"],
                "move-then-kill" => &["unlock:a", "unlock:a"],
                "kill-then-move" => &["unlock:a", "unlock:a", "unlock:b"],
                "kill-a-then-b" => &[
                    "unlock:a",
                    "unlock:a",
                    "unlock:b",
                    "unlock:b:dead",
                    "unlock:c",
                ],
                "kill-b-then-a" => &["unlock:a", "unlock:b", "unlock:a", "unlock:c"],
                "indirect-modify-after" => &["unlock:a", "unlock:a", "unlock:b:dead", "unlock:c"],
                "indirect-clear-after" => &["unlock:a", "unlock:a", "unlock:b", "unlock:c"],
                "unlock-throw" => &[
                    "list-before:t",
                    "kill-hook:1",
                    "unlock:a:hooks=1",
                    "after-catch:escaped:list=t:hooks=1",
                    "second-kill",
                    "unlock:a:hooks=1",
                ],
                "post-dying-state" => &[
                    "sentinel:positive",
                    "positive:sentinels=1",
                    "list-before:t",
                    "kill-hook:1",
                    "unlock:a:hooks=1:sentinels=1:pending-live=nil",
                    "after-catch:escaped:list=t:hooks=1:sentinels=1",
                    "after-escape:sentinels=1",
                    "second-kill",
                    "unlock:a:hooks=1:sentinels=1:pending-live=nil",
                ],
                _ => panic!("unknown shutdown unlock mutation case: {kind}"),
            },
            Self::DaemonUnixListener => &["user-unix-listener-created"],
            #[cfg(target_os = "linux")]
            Self::IgnoredChildHangup(_) => &[],
            Self::PipeServerSentinels => &[
                "sentinel:control-pipe",
                "sentinel:control-server",
                "kill-hook",
                "autosave-old-status:closed/closed",
                "after-new-delete",
            ],
            Self::KillBufferUnlock(kind) => match kind {
                "real-hup" => &[
                    "READY",
                    "unlock-outer",
                    "unlock-inner",
                    "inner-return=t",
                    "ACK-1=1",
                    "outer-return=t",
                    "ACK-2=2",
                ],
                "current-indirect-frame-preference" => &[
                    "BEFORE (:current-child t :window \"selection-shown\" :other \"selection-preferred\" :different t)",
                    "AFTER (:result t :current-live t :current \"selection-preferred\" :window \"selection-shown\" :current-eq-before-other t :current-eq-window nil)",
                    "DEAD-OTHER (:child-live nil :other \"selection-preferred\" :same-as-before t)",
                ],
                "current-indirect-base" => &[
                    "SELECTION (:before-child t :kill-result t :base-live nil :child-live nil :current-live t :current-name \"*scratch*\" :selected-window-live t :current-matches-window t)",
                ],
                "normal" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "current" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "function-cell" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "unmodified" => &[
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "nil-truename" => &[
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "current-reselect" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                    "after-selection:third=t",
                ],
                "kill-target" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "unlock:/inner-unlock-probe/truename:armed=nil:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-first:result=t:target-live=nil:pipe-live=nil:current-target=nil",
                ],
                "throw" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "after-first:result=escaped:target-live=t:pipe-live=t:current-target=nil",
                    "unlock:/inner-unlock-probe/truename:armed=nil:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-second:result=t:target-live=nil:pipe-live=nil",
                ],
                "current-throw" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "after-first:result=escaped:target-live=t:pipe-live=t:current-target=nil",
                    "unlock:/inner-unlock-probe/truename:armed=nil:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-second:result=t:target-live=nil:pipe-live=nil",
                ],
                "indirect-throw" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "after-first:result=escaped:target-live=t:pipe-live=t:current-target=nil",
                    "unlock:/inner-unlock-probe/truename:armed=nil:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-second:result=t:target-live=nil:pipe-live=nil",
                ],
                "file-error" => &[
                    "unlock:/inner-unlock-probe/truename:armed=t:target-live=t:current-target=nil:pipe-live=t",
                    "after-first:result=(caught file-error):target-live=t:pipe-live=t:current-target=nil",
                    "unlock:/inner-unlock-probe/truename:armed=nil:target-live=t:current-target=nil:pipe-live=t",
                    "sentinel:target-live=t",
                    "after-second:result=t:target-live=nil:pipe-live=nil",
                ],
                _ => panic!("unknown kill-buffer unlock case: {kind}"),
            },
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
            Self::HookThrowAbort => &[
                "HOOK-THROW",
                "RETURN=thrown",
                "STILL-ALIVE",
                "SECOND-KILL-HOOK",
            ],
            Self::HookOuterHandler => &["HOOK-BAD", "HOOK-NEXT=(probe-next)"],
            Self::HookNoCatch => &["HOOK-NOCATCH", "HOOK-NEXT=(probe-next)"],
            Self::HookUnintern => &[
                "SETUP original=t replacement=nil distinct=t",
                "ORIGINAL-HOOK",
            ],
            Self::SafeHookDebugger => &["next"],
            Self::SafeHookDebugOnSignal => &["debugger", "next"],
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
            exit: match self {
                Self::ListPostlude("signal-term") => Some(libc::SIGTERM),
                Self::ListPostlude("signal-abort") => None,
                _ => Some(0),
            },
            signal: matches!(self, Self::ListPostlude("signal-abort")).then_some(libc::SIGABRT),
            artifact_before: self.checkpoint().then_some(true),
            artifact_after: match self {
                Self::ListPostlude(
                    "postlude-expand-escape"
                    | "postlude-input-nul"
                    | "postlude-missing"
                    | "postlude-directory"
                    | "save-escape"
                    | "nonstr-nil"
                    | "nonstr-t"
                    | "nonstr-int"
                    | "nonstr-cons"
                    | "signal-abort",
                ) => Some(true),
                _ => self
                    .checkpoint()
                    .then_some(matches!(self, Self::DaemonUnixListener)),
            },
            secondary_after: match self {
                Self::ListPostlude("postlude-directory" | "unintern-nil" | "unintern-decoy") => {
                    Some(true)
                }
                Self::ListPostlude("postlude-missing") => Some(false),
                _ => None,
            },
            events: events.iter().map(|s| (*s).into()).collect(),
            #[cfg(target_os = "linux")]
            native_child: match self {
                Self::KillBufferUnlock("real-hup") => Some(NativeObservation::CaughtHup {
                    disposition_before_go: true,
                    hup_acknowledgements: 2,
                    child_exited_before_cleanup: true,
                }),
                Self::IgnoredChildHangup(kind) => Some(NativeObservation::IgnoredHup {
                    disposition_before_go: true,
                    child_alive_after_editor_exit: !matches!(
                        kind,
                        IgnoredHupProbe::ExplicitDeleteControl
                    ),
                    child_alive_later: !matches!(kind, IgnoredHupProbe::ExplicitDeleteControl),
                }),
                _ => None,
            },
        }
    }
}

struct RunningEditor {
    child: Child,
    root: PathBuf,
    #[cfg(target_os = "linux")]
    native_child: Option<NativeChild>,
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
        #[cfg(target_os = "linux")]
        if let Some(child) = &self.native_child {
            // Editor is stopped/reaped first. Adopted children retain their
            // captured pidfd identity; never signal a recorded numeric PID.
            if let Err(error) = child.cleanup() {
                eprintln!("owned native child cleanup failed: {error}");
            }
        }
        // A setup failure before capture remains independently bounded:
        // helpers watch the editor parent before GO and have a timeout.
    }
}

fn observe(probe: Probe, binary: &Path) -> Observation {
    let case = OracleSandbox::create_fixture_tempdir().unwrap();
    let root = case.path();
    let editor_mode = probe.editor_mode();
    // Use the existing bounded TempBase policy (<=32-byte base) so both
    // private socket paths fit Unix sun_path, independent of workspace depth.
    let socket_namespace = match editor_mode {
        EditorMode::Batch => None,
        EditorMode::ForegroundDaemon => Some(bounded_temp_dir("nv-shut-").unwrap()),
    };
    let target = root.join("visited.txt");
    let artifact = match probe {
        Probe::DaemonUnixListener => socket_namespace.as_ref().unwrap().path().join("user.sock"),
        Probe::Unlock => root.join(".#visited.txt"),
        _ => root.join("auto-save-list"),
    };
    fs::write(&target, "initial saved content\n").unwrap();
    if matches!(
        probe,
        Probe::AutoSaveList
            | Probe::ListPostlude(_)
            | Probe::UnlockMutation("unlock-throw" | "post-dying-state")
    ) {
        fs::write(&artifact, "preexisting auto-save session list\n").unwrap();
    }
    let secondary = probe.secondary_path(root);
    match probe {
        Probe::ListPostlude("unintern-nil" | "unintern-decoy") => {
            fs::write(secondary.as_ref().unwrap(), "native decoy must survive\n").unwrap();
        }
        Probe::ListPostlude("postlude-directory") => {
            fs::create_dir(secondary.as_ref().unwrap()).unwrap();
        }
        _ => {}
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
    #[cfg(target_os = "linux")]
    let native_spec = probe.native_child_spec();
    #[cfg(target_os = "linux")]
    if let Some(spec) = native_spec {
        prepare_native_child_supervision().expect("pidfd/P_PIDFD fixture prerequisites");
        fs::write(root.join("child.py"), spec.helper).unwrap();
    }
    let sandbox = OracleSandbox::new(
        probe.script(),
        &[],
        &common::oracle_sandbox::project_root().join("lisp"),
    )
    .unwrap();
    let mut command = if probe.signal().is_some() {
        // PinnedCommand intentionally has no pre_exec escape hatch. The fixed
        // child-only shell wrapper disables core files and execs this exact
        // editor argv, preserving the owned Child PID and final signal status.
        let mut command = sandbox.pinned_command("sh");
        command
            .args(["-c", "ulimit -c 0 && exec \"$@\"", "shutdown-signal"])
            .arg(binary);
        command
    } else {
        sandbox.pinned_command(binary)
    };
    sandbox.configure(&mut command);
    match editor_mode {
        EditorMode::Batch => {
            command.env_remove("SHUTDOWN_EXPECT_FOREGROUND_DAEMON");
            command.args(["--batch", "--quick", "--load"]);
        }
        EditorMode::ForegroundDaemon => {
            let directory = socket_namespace.as_ref().unwrap().path();
            let daemon_socket = directory.join("daemon.sock");
            assert!(
                daemon_socket.as_os_str().len() < 104,
                "bounded daemon socket path"
            );
            assert!(
                directory.join("user.sock").as_os_str().len() < 104,
                "bounded user socket path"
            );
            // A fresh absolute daemon name isolates the server from any
            // system/default server. OracleSandbox already pins private HOME.
            let mut daemon = OsString::from("--fg-daemon=");
            daemon.push(&daemon_socket);
            command.arg("--quick").arg(daemon).arg("--load");
            command.env("SHUTDOWN_PROBE_SOCKET_ROOT", directory);
            command.env("SHUTDOWN_EXPECT_FOREGROUND_DAEMON", "1");
        }
    }
    command
        .arg(&script)
        .env("SHUTDOWN_TARGET", &target)
        .env("SHUTDOWN_ARTIFACT", &artifact)
        .env("SHUTDOWN_SECONDARY", secondary.as_deref().unwrap_or(root))
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
    #[cfg(target_os = "linux")]
    if let Probe::IgnoredChildHangup(kind) = probe {
        match kind {
            IgnoredHupProbe::Pty => {
                command.env("SHUTDOWN_PROBE_PTY", "1");
            }
            IgnoredHupProbe::PtyWithStderr => {
                command.env("SHUTDOWN_PROBE_PTY", "1");
                command.env("SHUTDOWN_PROBE_SPLIT_STDERR", "1");
            }
            IgnoredHupProbe::ExplicitDeleteControl => {
                command.env("SHUTDOWN_PROBE_DELETE_CONTROL", "1");
            }
            IgnoredHupProbe::Pipe | IgnoredHupProbe::ForegroundDaemonPipe => {}
        }
    }
    if let Probe::KillBufferUnlock(kind) = probe {
        command.env("KILL_BUFFER_UNLOCK_CASE", kind);
    }
    if let Probe::ListPostlude(kind) = probe {
        command.env("SHUTDOWN_LIST_ROW", kind);
    }
    if let Probe::UnlockMutation(kind) = probe {
        command.env("SHUTDOWN_UNLOCK_CASE", kind);
    }
    let mut editor = RunningEditor {
        child: command.spawn().unwrap(),
        root: root.into(),
        #[cfg(target_os = "linux")]
        native_child: None,
    };
    let before = if probe.checkpoint() {
        editor.checkpoint(&root.join("ready"));
        let exists = fs::symlink_metadata(&artifact).is_ok();
        assert!(
            exists,
            "native artifact absent before kill: {}",
            editor.diagnostics()
        );
        if matches!(probe, Probe::DaemonUnixListener) {
            assert!(
                fs::symlink_metadata(&artifact)
                    .unwrap()
                    .file_type()
                    .is_socket(),
                "daemon user listener must be a native Unix socket"
            );
        }
        if matches!(probe, Probe::Unlock) {
            assert!(
                fs::symlink_metadata(&artifact)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
        if let Some(signal) = probe.signal() {
            let pid: libc::pid_t = editor
                .child
                .id()
                .try_into()
                .expect("owned child PID fits pid_t");
            // SAFETY: Child is owned here and has not been reaped, so its PID
            // cannot be reused for a different process before this kill call.
            assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
        } else {
            fs::write(root.join("go"), "go\n").unwrap();
        }
        Some(exists)
    } else {
        None
    };
    #[cfg(target_os = "linux")]
    if let Some(spec) = native_spec {
        editor.checkpoint(&root.join("ready"));
        editor.native_child = Some(
            NativeChild::capture(root, editor.child.id(), spec.disposition)
                .expect("capture owned native child before GO"),
        );
        editor
            .native_child
            .as_ref()
            .unwrap()
            .validate_before_go()
            .expect("native child disposition before GO");
        fs::write(root.join("go"), "go\n").unwrap();
    }
    let status = editor.wait();
    #[cfg(target_os = "linux")]
    let native_child = editor.native_child.as_ref().map(|child| {
        child
            .observe_after_editor_exit(root, native_spec.unwrap().after_editor)
            .expect("native child observation after editor exit")
    });
    #[cfg(target_os = "linux")]
    if let Some(child) = &editor.native_child {
        child.cleanup().expect("reap captured child through pidfd");
    }
    #[cfg(target_os = "linux")]
    {
        editor.native_child = None;
    }
    let events = match fs::read_to_string(root.join("events.log")) {
        Ok(text) => text.lines().map(str::to_owned).collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(error) => panic!("read shutdown observations: {error}"),
    };
    let result = Observation {
        exit: status.code(),
        signal: status.signal(),
        artifact_before: before,
        artifact_after: probe.checkpoint().then(|| {
            fs::symlink_metadata(&artifact).is_ok_and(|metadata| {
                !matches!(probe, Probe::DaemonUnixListener) || metadata.file_type().is_socket()
            })
        }),
        secondary_after: secondary
            .as_ref()
            .map(|path| fs::symlink_metadata(path).is_ok()),
        events,
        #[cfg(target_os = "linux")]
        native_child,
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

// These extend the observer set; the six original shutdown rows stay intact.
#[test]
fn shutdown_kill_hook_throw_aborts_before_dying_and_a_second_kill_succeeds() {
    assert_shutdown_parity(Probe::HookThrowAbort);
}

#[test]
fn shutdown_kill_hook_barrier_blocks_outer_handler_bind() {
    assert_shutdown_parity(Probe::HookOuterHandler);
}

#[test]
fn shutdown_kill_hook_no_catch_error_removes_function_and_continues() {
    assert_shutdown_parity(Probe::HookNoCatch);
}

// Raw subprocesses preserve the debugger's native signal context. The common
// value harness adds an outer condition-case that can mask this regression.
#[test]
fn safe_hook_barrier_suppresses_ordinary_debugger() {
    assert_shutdown_parity(Probe::SafeHookDebugger);
}

#[test]
fn safe_hook_barrier_preserves_debug_on_signal() {
    assert_shutdown_parity(Probe::SafeHookDebugOnSignal);
}

#[test]
fn shutdown_kill_hook_uses_original_symbol_after_unintern() {
    assert_shutdown_parity(Probe::HookUnintern);
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_keeps_hup_ignoring_pipe_child_alive() {
    assert_shutdown_parity(Probe::IgnoredChildHangup(IgnoredHupProbe::Pipe));
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_keeps_hup_ignoring_pty_child_alive() {
    assert_shutdown_parity(Probe::IgnoredChildHangup(IgnoredHupProbe::Pty));
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_keeps_hup_ignoring_pty_split_stderr_child_alive() {
    assert_shutdown_parity(Probe::IgnoredChildHangup(IgnoredHupProbe::PtyWithStderr));
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_explicit_delete_still_kills_hup_ignoring_child_control() {
    assert_shutdown_parity(Probe::IgnoredChildHangup(
        IgnoredHupProbe::ExplicitDeleteControl,
    ));
}

#[test]
fn shutdown_closes_pipe_and_server_before_autosave_without_sentinels() {
    assert_shutdown_parity(Probe::PipeServerSentinels);
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_foreground_daemon_keeps_hup_ignoring_child_alive() {
    assert_shutdown_parity(Probe::IgnoredChildHangup(
        IgnoredHupProbe::ForegroundDaemonPipe,
    ));
}

#[test]
fn shutdown_foreground_daemon_preserves_user_unix_listener_path() {
    assert_shutdown_parity(Probe::DaemonUnixListener);
}

#[test]
fn kill_buffer_unlock_normal() {
    assert_shutdown_parity(Probe::KillBufferUnlock("normal"));
}

#[test]
fn kill_buffer_unlock_current() {
    assert_shutdown_parity(Probe::KillBufferUnlock("current"));
}

#[test]
fn kill_buffer_unlock_function_cell() {
    assert_shutdown_parity(Probe::KillBufferUnlock("function-cell"));
}

#[test]
fn kill_buffer_unlock_unmodified() {
    assert_shutdown_parity(Probe::KillBufferUnlock("unmodified"));
}

#[test]
fn kill_buffer_unlock_nil_truename() {
    assert_shutdown_parity(Probe::KillBufferUnlock("nil-truename"));
}

#[test]
fn kill_buffer_unlock_current_reselect() {
    assert_shutdown_parity(Probe::KillBufferUnlock("current-reselect"));
}

#[test]
fn kill_buffer_unlock_kill_target() {
    assert_shutdown_parity(Probe::KillBufferUnlock("kill-target"));
}

#[test]
fn kill_buffer_unlock_throw() {
    assert_shutdown_parity(Probe::KillBufferUnlock("throw"));
}

#[test]
fn kill_buffer_unlock_current_throw() {
    assert_shutdown_parity(Probe::KillBufferUnlock("current-throw"));
}

#[test]
fn kill_buffer_unlock_indirect_throw() {
    assert_shutdown_parity(Probe::KillBufferUnlock("indirect-throw"));
}

#[test]
fn kill_buffer_unlock_file_error() {
    assert_shutdown_parity(Probe::KillBufferUnlock("file-error"));
}

#[test]
#[cfg(target_os = "linux")]
fn kill_buffer_unlock_reentrant_real_child_receives_both_hangups() {
    assert_shutdown_parity(Probe::KillBufferUnlock("real-hup"));
}

#[test]
fn kill_buffer_base_with_current_indirect_keeps_live_selection() {
    assert_shutdown_parity(Probe::KillBufferUnlock("current-indirect-base"));
}

// Preserve native frame preference during transitional indirect retirement.
#[test]
fn kill_buffer_current_indirect_keeps_native_frame_preference() {
    assert_shutdown_parity(Probe::KillBufferUnlock("current-indirect-frame-preference"));
}

// Each case reuses the same bounded external editor supervisor.
#[test]
fn shutdown_unlock_append() {
    assert_shutdown_parity(Probe::UnlockMutation("append"));
}

#[test]
fn shutdown_unlock_rename_later() {
    assert_shutdown_parity(Probe::UnlockMutation("rename-later"));
}

#[test]
fn shutdown_unlock_clear_later() {
    assert_shutdown_parity(Probe::UnlockMutation("clear-later"));
}

#[test]
fn shutdown_unlock_bury_current() {
    assert_shutdown_parity(Probe::UnlockMutation("bury-current"));
}

#[test]
fn shutdown_unlock_kill_current() {
    assert_shutdown_parity(Probe::UnlockMutation("kill-current"));
}

#[test]
fn shutdown_unlock_move_then_kill() {
    assert_shutdown_parity(Probe::UnlockMutation("move-then-kill"));
}

#[test]
fn shutdown_unlock_kill_then_move() {
    assert_shutdown_parity(Probe::UnlockMutation("kill-then-move"));
}

#[test]
fn shutdown_unlock_kill_a_then_b() {
    assert_shutdown_parity(Probe::UnlockMutation("kill-a-then-b"));
}

#[test]
fn shutdown_unlock_kill_b_then_a() {
    assert_shutdown_parity(Probe::UnlockMutation("kill-b-then-a"));
}

#[test]
fn shutdown_unlock_indirect_modify_after() {
    assert_shutdown_parity(Probe::UnlockMutation("indirect-modify-after"));
}

#[test]
fn shutdown_unlock_indirect_clear_after() {
    assert_shutdown_parity(Probe::UnlockMutation("indirect-clear-after"));
}

#[test]
fn shutdown_unlock_unlock_throw() {
    assert_shutdown_parity(Probe::UnlockMutation("unlock-throw"));
}

#[test]
fn shutdown_unlock_post_dying_state() {
    assert_shutdown_parity(Probe::UnlockMutation("post-dying-state"));
}

#[test]
fn shutdown_list_postlude_postlude_raw_bypass() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-raw-bypass"));
}

#[test]
fn shutdown_list_postlude_postlude_expand_function_cell() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-expand-function-cell"));
}

#[test]
fn shutdown_list_postlude_postlude_expand_handler() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-expand-handler"));
}

#[test]
fn shutdown_list_postlude_postlude_expand_escape() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-expand-escape"));
}

#[test]
fn shutdown_list_postlude_postlude_missing() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-missing"));
}

#[test]
fn shutdown_list_postlude_postlude_directory() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-directory"));
}

#[test]
fn shutdown_list_postlude_save_escape() {
    assert_shutdown_parity(Probe::ListPostlude("save-escape"));
}

#[test]
fn shutdown_list_postlude_nonstr_nil() {
    assert_shutdown_parity(Probe::ListPostlude("nonstr-nil"));
}

#[test]
fn shutdown_list_postlude_nonstr_t() {
    assert_shutdown_parity(Probe::ListPostlude("nonstr-t"));
}

#[test]
fn shutdown_list_postlude_nonstr_int() {
    assert_shutdown_parity(Probe::ListPostlude("nonstr-int"));
}

#[test]
fn shutdown_list_postlude_nonstr_cons() {
    assert_shutdown_parity(Probe::ListPostlude("nonstr-cons"));
}

#[test]
fn shutdown_list_postlude_unintern_nil() {
    assert_shutdown_parity(Probe::ListPostlude("unintern-nil"));
}

#[test]
fn shutdown_list_postlude_unintern_decoy() {
    assert_shutdown_parity(Probe::ListPostlude("unintern-decoy"));
}

#[test]
fn shutdown_list_postlude_signal_term() {
    assert_shutdown_parity(Probe::ListPostlude("signal-term"));
}

#[test]
fn shutdown_list_postlude_signal_abort() {
    assert_shutdown_parity(Probe::ListPostlude("signal-abort"));
}

#[test]
fn shutdown_list_postlude_expand_handler_result_nul_uses_c_string_prefix() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-expand-nul"));
}

#[test]
fn shutdown_list_postlude_input_nul_rejects_before_handler_and_preserves_list() {
    assert_shutdown_parity(Probe::ListPostlude("postlude-input-nul"));
}
