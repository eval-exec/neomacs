//! Linux native child identity for shutdown fixtures. No PID-based cleanup.
#![cfg(target_os = "linux")]

use std::{
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HupDisposition {
    Ignored,
    Caught,
    /// Neither caught, ignored, nor blocked: the child dies if SIGHUP is
    /// delivered, exactly like a plain `telega-server` class subprocess.
    Default,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChildAfterEditor {
    Alive,
    Exited,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NativeObservation {
    IgnoredHup {
        disposition_before_go: bool,
        child_alive_after_editor_exit: bool,
        child_alive_later: bool,
    },
    CaughtHup {
        disposition_before_go: bool,
        hup_acknowledgements: usize,
        child_exited_before_cleanup: bool,
    },
    KilledByHup {
        disposition_before_go: bool,
        /// The reaped status records death by the delivered SIGHUP itself,
        /// not SIGKILL, an ordinary exit, or a different signal.
        exited_by_sighup: bool,
        /// The child read stdin to EOF; a GNU-faithful shutdown delivers
        /// SIGHUP before Emacs closes the pipe, so this stays false.
        eof_observed: bool,
    },
}

/// Where the captured child's terminal status comes from.
///
/// A waitid(P_PIDFD) can lose the race to the editor's own SIGCHLD handler,
/// which reaps its child before the fixture observes it (ECHILD).  The exit
/// evidence still exists:
///
/// * `PidfdInfo` -- PIDFD_GET_INFO with PIDFD_INFO_EXIT (Linux >= 6.15)
///   returns the wait(2)-style status after the task has been reaped by
///   anyone, for as long as the pidfd is open.
/// * `Ptrace` -- the fixture seizes the descendant with PTRACE_O_TRACEEXIT
///   before GO.  The exit-event stop precedes the real death, and the real
///   wait(2) status follows it, so the status cannot be taken by the editor
///   first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitSource {
    PidfdInfo,
    Ptrace,
}

/// Linux >= 6.15 `struct pidfd_info` (UAPI: include/linux/pidfd.h).
/// `mask` is the request on input and the filled-fields report on output.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PidfdInfo {
    mask: u64,
    cgroupid: u64,
    pid: u32,
    tgid: u32,
    ppid: u32,
    ruid: u32,
    rgid: u32,
    euid: u32,
    egid: u32,
    suid: u32,
    sgid: u32,
    fsuid: u32,
    fsgid: u32,
    exit_code: i32,
    coredump_mask: u32,
    spare1: u32,
}

const PIDFD_INFO_EXIT: u64 = 1 << 3;

/// `_IOWR(PIDFS_IOCTL_MAGIC, 11, struct pidfd_info)`: direction bits 3,
/// size in bits 16.., type 0xFF, nr 11 (generic Linux ioctl encoding, the
/// only one this Linux x86_64/aarch64 harness is built for).
const PIDFD_GET_INFO: libc::c_ulong =
    ((3u64 << 30) | ((std::mem::size_of::<PidfdInfo>() as u64) << 16) | ((0xFFu64) << 8) | 11)
        as libc::c_ulong;

/// `None` means "the kernel has not recorded the exit yet"; keep polling.
fn pidfd_exit_status(fd: &OwnedFd) -> io::Result<Option<i32>> {
    let mut info = PidfdInfo {
        mask: PIDFD_INFO_EXIT,
        ..PidfdInfo::default()
    };
    // SAFETY: `info` is a valid, uniquely borrowed pidfd_info for the ioctl;
    // the pidfd is owned by the caller for the whole call.
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), PIDFD_GET_INFO, &mut info) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error);
    }
    if info.mask & PIDFD_INFO_EXIT == 0 {
        return Ok(None);
    }
    Ok(Some(info.exit_code))
}

fn pidfd_info_supported() -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let Ok(fd) = pidfd_open(std::process::id()) else {
            return false;
        };
        pidfd_exit_status(&fd).is_ok()
    })
}

/// Test-only override that forces the PTRACE_O_TRACEEXIT source, so the
/// fallback is executable on kernels that also provide PIDFD_INFO_EXIT.
fn force_ptrace_exit_source() -> bool {
    std::env::var_os("NEOVM_TEST_FORCE_PTRACE_EXIT").is_some()
}

fn selected_exit_source() -> ExitSource {
    if force_ptrace_exit_source() {
        ExitSource::Ptrace
    } else if pidfd_info_supported() {
        ExitSource::PidfdInfo
    } else {
        ExitSource::Ptrace
    }
}

fn is_ptrace_exit_stop(status: i32) -> bool {
    (status >> 8) as i32 == (libc::SIGTRAP | ((libc::PTRACE_EVENT_EXIT as i32) << 8))
}

/// Seize the descendant with exit-event tracing.  The child is the editor's
/// child and this test process is its subreaper, so Yama's ancestry rule
/// permits the attach.
fn ptrace_seize_exit(pid: u32) -> io::Result<()> {
    // SAFETY: integer pid; null addr; the options value is passed as data.
    let result = unsafe {
        libc::ptrace(
            libc::PTRACE_SEIZE as libc::c_uint,
            pid as libc::pid_t,
            std::ptr::null_mut::<libc::c_void>(),
            libc::PTRACE_O_TRACEEXIT as usize as *mut libc::c_void,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Wait for the traced child's real terminal wait(2) status, continuing past
/// the PTRACE_EVENT_EXIT stop.  Returns None when `deadline` elapses first.
fn wait_traced_exit(pid: u32, deadline: Instant) -> io::Result<Option<i32>> {
    loop {
        let mut status: libc::c_int = 0;
        // SAFETY: `status` is writable storage for one wait status.
        let result = unsafe {
            libc::waitpid(
                pid as libc::pid_t,
                &mut status,
                libc::__WALL | libc::WNOHANG,
            )
        };
        if result == pid as libc::pid_t {
            if libc::WIFSTOPPED(status) {
                // PTRACE_SEIZE reports a signal-delivery stop first, then the
                // PTRACE_EVENT_EXIT stop.  Forward the delivery stop's signal
                // (a bare PTRACE_CONT would suppress SIGHUP itself); the exit
                // event resumes without one.
                let deliver = if is_ptrace_exit_stop(status) {
                    0
                } else {
                    libc::WSTOPSIG(status)
                };
                // SAFETY: the tracee is stopped; this call resumes it with
                // the selected signal (0 = none).
                let resumed = unsafe {
                    libc::ptrace(
                        libc::PTRACE_CONT as libc::c_uint,
                        pid as libc::pid_t,
                        std::ptr::null_mut::<libc::c_void>(),
                        deliver as usize as *mut libc::c_void,
                    )
                };
                if resumed < 0 {
                    return Err(io::Error::last_os_error());
                }
                continue;
            }
            return Ok(Some(status));
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(2));
    }
}

/// Reap a traced child; an already-consumed status is a clean ECHILD.
fn reap_traced(pid: u32, deadline: Instant) -> io::Result<()> {
    loop {
        match wait_traced_exit(pid, Instant::now()) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            // The status was already consumed by this observation.
            Err(error) if error.raw_os_error() == Some(libc::ECHILD) => return Ok(()),
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "ptrace fixture reap",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn status_terminating_signal(status: i32) -> Option<i32> {
    if libc::WIFSIGNALED(status) {
        Some(libc::WTERMSIG(status))
    } else {
        None
    }
}

/// One blocking waitid(P_PIDFD, WEXITED) decoded to (si_code, si_status).
fn waitid_exited_status(fd: &OwnedFd) -> io::Result<(libc::c_int, libc::c_int)> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: zero is a valid initial siginfo_t; the child has already
        // exited, so this blocking waitid returns at once.
        let result = unsafe {
            libc::waitid(
                libc::P_PIDFD,
                fd.as_raw_fd() as libc::id_t,
                &mut info,
                libc::WEXITED,
            )
        };
        if result == 0 {
            // SAFETY: successful waitid initialized these SIGCHLD members.
            return Ok((info.si_code, unsafe { info.si_status() }));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn pidfd_open(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: integer syscall arguments; success creates one new descriptor.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0u32) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: syscall success returned a valid, uniquely owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

pub(super) fn prepare_native_child_supervision() -> io::Result<()> {
    // Verify both pidfd_open and P_PIDFD support before spawning an editor.
    let own = NativeChild {
        fd: pidfd_open(std::process::id())?,
        pid: std::process::id(),
        exit_source: ExitSource::PidfdInfo,
        disposition: HupDisposition::Ignored,
        disposition_before_go: false,
    };
    own.reap_if_exited()?; // ECHILD is expected for this process itself.
    // Process-wide test-only setting; nextest isolates each test process.
    // Never disable it while an owned editor could orphan its child.
    // SAFETY: PR_SET_CHILD_SUBREAPER takes integer arguments, no pointer.
    if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) struct NativeChild {
    fd: OwnedFd,
    pid: u32,
    exit_source: ExitSource,
    disposition: HupDisposition,
    disposition_before_go: bool,
}

impl NativeChild {
    /// Capture while the editor is held at READY and before supervisor GO.
    pub(super) fn capture(
        root: &Path,
        editor_pid: u32,
        disposition: HupDisposition,
    ) -> io::Result<Self> {
        let ready = fs::read_to_string(root.join("child.ready"))?;
        let identity = ready
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(io::Error::other)?;
        if identity.len() != 4
            || identity[0] == 0
            || identity[0] > i32::MAX as u32
            || identity[1] != editor_pid
            || identity[2] != identity[0]
            || identity[3] != identity[0]
        {
            return Err(io::Error::other("native child readiness identity mismatch"));
        }
        let pid = identity[0];
        let fd = pidfd_open(pid)?;
        let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .map(str::trim)
        };
        let parent = field("PPid:")
            .ok_or_else(|| io::Error::other("missing PPid"))?
            .parse::<u32>()
            .map_err(io::Error::other)?;
        if parent != editor_pid {
            return Err(io::Error::other(
                "captured process is not this editor's child",
            ));
        }
        // SAFETY: read-only group/session queries for a positive captured PID.
        let group = unsafe { libc::getpgid(pid as libc::pid_t) };
        let session = unsafe { libc::getsid(pid as libc::pid_t) };
        if group != pid as libc::pid_t || session != pid as libc::pid_t {
            return Err(io::Error::other(
                "child is not its own group/session leader",
            ));
        }
        let bits = |name: &str| -> io::Result<u64> {
            u64::from_str_radix(
                field(name).ok_or_else(|| io::Error::other("missing signal field"))?,
                16,
            )
            .map_err(io::Error::other)
        };
        let disposition_before_go = match disposition {
            HupDisposition::Ignored => bits("SigIgn:")? & 1 != 0,
            HupDisposition::Caught => bits("SigCgt:")? & 1 != 0 && bits("SigIgn:")? & 1 == 0,
            HupDisposition::Default => {
                bits("SigBlk:")? & 1 == 0 && bits("SigIgn:")? & 1 == 0 && bits("SigCgt:")? & 1 == 0
            }
        };
        // Arm the exit-status source whose evidence survives a reap by the
        // editor.  Done before GO: the child must not die before it is armed.
        // Only the default-disposition probe needs an exit status; the
        // ignored/caught probes observe aliveness or acknowledgements, and a
        // ptrace seize there would intercept the signals the editor sends
        // (they must reach the child, not stop it under an unserviced tracer).
        let exit_source = if disposition == HupDisposition::Default {
            selected_exit_source()
        } else {
            ExitSource::PidfdInfo
        };
        if exit_source == ExitSource::Ptrace {
            ptrace_seize_exit(pid)?;
        }
        Ok(Self {
            fd,
            pid,
            exit_source,
            disposition,
            disposition_before_go,
        })
    }

    /// Call only after storing the guard in RunningEditor, so assertion
    /// unwinding still owns cleanup for this captured process.
    pub(super) fn validate_before_go(&self) -> io::Result<()> {
        if !self.disposition_before_go || !self.is_alive()? {
            return Err(io::Error::other(format!(
                "native child must have {:?} HUP disposition and be alive before GO",
                self.disposition
            )));
        }
        Ok(())
    }

    fn is_alive(&self) -> io::Result<bool> {
        loop {
            let mut fd = libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: pointer refers to one initialized pollfd for this call.
            if unsafe { libc::poll(&mut fd, 1, 0) } < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if fd.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err(io::Error::other("invalid pidfd poll result"));
            }
            return Ok(fd.revents & (libc::POLLIN | libc::POLLHUP) == 0);
        }
    }

    pub(super) fn observe_after_editor_exit(
        &self,
        root: &Path,
        expected_child: ChildAfterEditor,
    ) -> io::Result<NativeObservation> {
        // Controls that explicitly deleted the child may have queued SIGKILL
        // immediately before editor exit. Bound settling before recording the
        // native result; this never waits for the helper's 30s self-timeout.
        if expected_child == ChildAfterEditor::Exited {
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.is_alive()? && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
        }
        if self.disposition == HupDisposition::Ignored {
            let child_alive_after_editor_exit = self.is_alive()?;
            thread::sleep(Duration::from_millis(150));
            return Ok(NativeObservation::IgnoredHup {
                disposition_before_go: self.disposition_before_go,
                child_alive_after_editor_exit,
                child_alive_later: self.is_alive()?,
            });
        }
        if self.disposition == HupDisposition::Default {
            let deadline = Instant::now() + Duration::from_secs(2);
            let exited_by_sighup = match self.exit_source {
                ExitSource::PidfdInfo => {
                    while self.is_alive()? && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(5));
                    }
                    if self.is_alive()? {
                        return Err(io::Error::other(
                            "default-disposition child still alive after editor exit",
                        ));
                    }
                    // The fixture normally reaps here so the status is
                    // attributable to this captured process.  The editor's
                    // own SIGCHLD handler may have reaped it first (ECHILD);
                    // the pidfd still carries the recorded exit status.
                    match waitid_exited_status(&self.fd) {
                        Ok((code, status)) => code == libc::CLD_KILLED && status == libc::SIGHUP,
                        Err(error) if error.raw_os_error() == Some(libc::ECHILD) => {
                            let recorded = loop {
                                if let Some(status) = pidfd_exit_status(&self.fd)? {
                                    break Some(status);
                                }
                                if Instant::now() >= deadline {
                                    break None;
                                }
                                thread::sleep(Duration::from_millis(2));
                            };
                            let Some(status) = recorded else {
                                return Err(io::Error::other(
                                    "child was reaped before observation and the pidfd                                      recorded no exit status (kernel without PIDFD_INFO_EXIT?)",
                                ));
                            };
                            status_terminating_signal(status) == Some(libc::SIGHUP)
                        }
                        Err(error) => return Err(error),
                    }
                }
                ExitSource::Ptrace => {
                    let Some(status) = wait_traced_exit(self.pid, deadline)? else {
                        return Err(io::Error::other(
                            "default-disposition child still alive after editor exit",
                        ));
                    };
                    status_terminating_signal(status) == Some(libc::SIGHUP)
                }
            };
            return Ok(NativeObservation::KilledByHup {
                disposition_before_go: self.disposition_before_go,
                exited_by_sighup,
                eof_observed: root.join("child.eof").exists(),
            });
        }
        let acknowledgements = fs::read_to_string(root.join("hup-acks"))?;
        if acknowledgements.lines().any(|line| line != "HUP") {
            return Err(io::Error::other(
                "invalid native HUP acknowledgement record",
            ));
        }
        Ok(NativeObservation::CaughtHup {
            disposition_before_go: self.disposition_before_go,
            hup_acknowledgements: acknowledgements.lines().count(),
            child_exited_before_cleanup: !self.is_alive()?,
        })
    }

    fn send_signal(&self, signal: i32) -> io::Result<()> {
        // SAFETY: owned pidfd identifies only the captured process; null
        // siginfo requests ordinary delivery. No numeric PID fallback exists.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }

    fn reap_if_exited(&self) -> io::Result<bool> {
        loop {
            // SAFETY: zero is a valid initial siginfo_t; waitid initializes it.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PIDFD,
                    self.fd.as_raw_fd() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                )
            };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                // An editor-reaped positive control is already clean. Never
                // retry using a recorded PID, which could now be reused.
                return if error.raw_os_error() == Some(libc::ECHILD) {
                    Ok(true)
                } else {
                    Err(error)
                };
            }
            // SAFETY: successful waitid initialized this SIGCHLD union member.
            return Ok(unsafe { info.si_pid() } != 0);
        }
    }

    /// RunningEditor must stop/reap its editor before invoking this method,
    /// so any surviving grandchild has been adopted by the test subreaper.
    pub(super) fn cleanup(&self) -> io::Result<()> {
        if self.is_alive()? {
            self.send_signal(libc::SIGTERM)?;
            let deadline = Instant::now() + Duration::from_millis(250);
            while self.is_alive()? && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if self.is_alive()? {
                self.send_signal(libc::SIGKILL)?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        if self.exit_source == ExitSource::Ptrace {
            reap_traced(self.pid, deadline)?;
            return Ok(());
        }
        while !self.reap_if_exited()? {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "pidfd fixture reap",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}
