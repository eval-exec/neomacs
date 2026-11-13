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
        Ok(Self {
            fd,
            disposition,
            disposition_before_go: match disposition {
                HupDisposition::Ignored => bits("SigIgn:")? & 1 != 0,
                HupDisposition::Caught => bits("SigCgt:")? & 1 != 0 && bits("SigIgn:")? & 1 == 0,
            },
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
