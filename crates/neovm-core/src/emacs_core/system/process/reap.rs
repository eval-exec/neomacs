//! The single owner of `waitpid` in this port, and GNU's `p->alive` as a type
//! rather than a `bool`.
//!
//! # GNU has exactly one `waitpid`, and says why
//!
//! `grep -n 'waitpid' src/*.c` over the GNU tree returns **one** call in the
//! whole POSIX build of the editor: `src/sysdep.c:471`, inside
//! `get_child_status`, which is `static`.  Everything reaches it through two
//! wrappers -- `child_status_changed` (`WNOHANG`, :515) and
//! `wait_for_termination` (blocking, :500) -- and it opens with
//!
//! ```c
//!   /* Invoke waitpid only with a known process ID; do not invoke
//!      waitpid with a nonpositive argument.  Otherwise, Emacs might
//!      reap an unwanted process by mistake.  For example, invoking
//!      waitpid (-1, ...) can mess up glib by reaping glib's subprocesses,
//!      so that another thread running glib won't find them.  */
//!   eassert (child > 0);                             /* src/sysdep.c:456-462 */
//! ```
//!
//! The rule the choke point exists to keep is stated in full one file over,
//! above `deleted_pid_list` (src/process.c:1080-1088):
//!
//! ```text
//!    The main Emacs thread invokes waitpid only on child processes that
//!    it creates and that have not been reaped.  This avoid races on
//!    platforms such as GTK, where other threads create their own
//!    subprocesses which the main thread should not reap.  For example,
//!    if the main thread attempted to reap an already-reaped child, it
//!    might inadvertently reap a GTK-created process that happened to
//!    have the same process ID.
//! ```
//!
//! Two clauses, and they are different obligations.  *"that it creates"* is
//! about the population, and GNU keeps it with three enumerated registries
//! (`Vprocess_alist`, `deleted_pid_list`, and `Fcall_process`'s local `PID`).
//! ***"and that have not been reaped"* is about time**, and GNU keeps it with
//! one bit -- `p->alive`, cleared at the instant the reap happens
//! (`handle_child_signal`, src/process.c:7754).
//!
//! # Where that bit is READ, which is the half this port got wrong
//!
//! GNU never re-asks the OS whether a child is still alive.
//! `record_kill_process` (src/callproc.c:196-211) is the whole of GNU's
//! "terminate this child" operation:
//!
//! ```c
//!   /* If P is reapable, record it as a deleted process and kill it.
//!      Do this in a critical section.  */
//!   block_child_signal (&oldset);
//!   if (p->alive)
//!     {
//!       record_deleted_pid (p->pid, tempfile);
//!       p->alive = 0;
//!       kill (- p->pid, SIGKILL);
//!     }
//!   unblock_child_signal (&oldset);
//! ```
//!
//! -- a recorded bit, tested inside a critical section so the SIGCHLD handler
//! cannot clear it between the test and the `kill`.  `Fdelete_process` reaches
//! it as `if (p->alive) record_kill_process (p, Qnil);`
//! (src/process.c:1134-1135).
//!
//! And it is not only the delete path.  `process.c` contains exactly **two**
//! `kill` calls, and the one every signal subr reaches -- `kill-process`,
//! `interrupt-process`, `stop-process`, `quit-process`, `continue-process` --
//! is gated the same way, with the hazard spelled out
//! (`process_send_signal`, :7199-7205):
//!
//! ```c
//!   /* Do not kill an already-reaped process, as that could kill an
//!      innocent bystander that happens to have the same process ID.  */
//!   sigset_t oldset;
//!   block_child_signal (&oldset);
//!   if (p->alive)
//!     kill (pid, signo);
//!   unblock_child_signal (&oldset);
//! ```
//!
//! The other (`Finternal_default_signal_process`, :7399) is GNU's documented
//! *"PROCESS may also be a number ... the process need not be a child of this
//! Emacs"* path and is deliberately ungated -- including when it takes the pid
//! from a process object at :7377.  This port routes that subr through
//! `deliver_process_signal` too, so it is gated here where GNU's is not; the
//! answers coincide (`kill` on a reaped pid fails `ESRCH`) except when the pid
//! has been recycled, which is the case :7199's comment is about.
//!
//! This port had no such bit.  `LiveProcessIo::terminate_and_reap_children`
//! spelled the same guard as a fresh probe:
//!
//! ```ignore
//!   if let Some(child) = self.child.as_mut()
//!       && !matches!(child.try_wait(), Ok(Some(_)))
//!   {
//!       sys::send_signal_to_group(child.id() as i64, signal_kill_number());
//!       let _ = child.wait();
//!   }
//! ```
//!
//! and a probe cannot answer that question, for two reasons that compound.
//! `Child::try_wait` **is itself a `waitpid`**, so on a child this port had
//! already reaped through `sys::poll_child_status` it is the second reap GNU's
//! comment forbids; and its answer there is `Err(ECHILD)` -- *"nobody has that
//! child"* -- which `!matches!(.., Ok(Some(_)))` reads as **"still running"**
//! and follows with `kill(-pid, SIGKILL)` on a pid the kernel has handed back.
//!
//! # The type
//!
//! [`ChildOwnership`] is `p->alive` with the pid attached to the live arm:
//!
//! * [`ChildOwnership::Unreaped`] holds a [`ReapableChild`], whose fields are
//!   private to this module.  **Every reap in `neovm-core`'s process table is
//!   one of the three methods on it**, so a second reaper is not something to
//!   remember not to write -- there is no handle to write it against.
//! * [`ChildOwnership::Reaped`] carries the terminal status and **no pid**, so
//!   `kill` on a reaped pid is not a case that is checked for: it is a
//!   sentence with no subject.
//! * every transition to `Reaped` happens *inside* the operation that
//!   performed the reap, so the bit cannot be forgotten the way a hand-set
//!   `bool` can.
//!
//! GNU's critical section has no analogue here and needs none: this port's
//! reap runs on the Lisp thread, and its signal handlers are restricted to a
//! counter bump and a self-pipe `write` (ledger 184's `os_signal.rs`), so
//! nothing can clear the bit between the test and the `kill`.
//!
//! The honest limit, stated the way `os_signal.rs` states its own: Rust does
//! not stop a future author from writing a fresh `libc::waitpid` elsewhere.
//! What this buys is that the *existing* reaping sites in the process table
//! collapse to one owner and that the owner cannot be reached without moving
//! through the state machine.

use super::{Value, process_status_exit_value};

/// A child this port spawned and has **not** reaped: GNU's `p->alive == 1`,
/// carrying the pid that authorises `waitpid` and `kill` on it.
///
/// The fields are private and the only constructors are
/// [`ChildOwnership::of_pipe_child`] and [`ChildOwnership::of_pty_child`], so
/// the handles whose `try_wait` is a reap are not reachable from outside this
/// module.
pub(crate) struct ReapableChild {
    /// GNU's `p->pid` as a CAPABILITY.
    ///
    /// `Process::os_pid` keeps the same number for `Fprocess_id`, which GNU
    /// also keeps after the reap; this copy is the one that authorises a
    /// syscall, and it disappears with the reap.
    pid: u32,
    handle: ChildHandle,
}

/// Which kind of child handle a spawn left behind.
///
/// Both own a reap of their own -- `std::process::Child::try_wait` and
/// `portable_pty::Child::try_wait` are each a `waitpid` -- which is exactly
/// why neither may be reachable from anywhere else.
enum ChildHandle {
    /// `std::process::Child`: the pipe spawn, and the pty spawn whose
    /// `:stderr` split goes through `pre_exec`.
    Pipe(Box<std::process::Child>),
    /// `portable_pty`'s child handle.
    Pty(Box<dyn portable_pty::Child + Send + Sync>),
}

/// What one probe of a child's status found.
///
/// The three arms are GNU's three outcomes at `handle_child_signal`
/// (src/process.c:7741-7757): no change; a change that leaves the child alive
/// (`WIFSTOPPED`/`WIFCONTINUED`, which is why GNU passes
/// `WUNTRACED | WCONTINUED`); and a change that clears `p->alive`
/// (`WIFSIGNALED (status) || WIFEXITED (status)`, :7750-7752).
#[derive(Debug)]
pub(crate) enum ChildStatusChange {
    /// `waitpid` returned 0: nothing to record.
    NoChange,
    /// A transition that leaves the pid this port's: `stop`, or `run` again
    /// after a continue.
    StillOurs(Value),
    /// A terminal transition.  The pid has gone back to the OS and this
    /// [`ChildOwnership`] is now [`ChildOwnership::Reaped`].
    Reaped(Value),
    /// `ECHILD`: **nobody** has that child, so there is a pid to give up and
    /// no status to record.
    ///
    /// Under [`ChildOwnership`] this can only mean an out-of-process reaper
    /// got there first -- GNU's `lib_child_handler` case
    /// (src/process.c:7654-7660, Glib).  GNU's own answer at that point is a
    /// negative `get_child_status` return and the comment *"Most likely,
    /// waitpid is buggy and the operating system lost track of the child
    /// somehow"* (src/sysdep.c:473-479), so no status is invented here
    /// either; what changes is only that the pid stops being spellable.  It
    /// used to be read as "no change", which left the delete path signalling
    /// a pid this port did not own.
    Gone,
}

/// GNU's `p->alive`, with the pid attached to the arm that may use it.
///
/// [`Default`] is [`Self::NotAChild`], which is what ledger 165's finding
/// needs: a pipe, network or serial connection has no pid at all, so
/// `get_child_status`'s `eassert (child > 0)` (src/sysdep.c:462) is a state
/// this enum cannot enter rather than a check inside a loop.
#[derive(Default)]
pub(crate) enum ChildOwnership {
    /// No child was ever spawned: `make-pipe-process`,
    /// `make-network-process`, `make-serial-process`, and the harness-only
    /// records with no OS backing.
    #[default]
    NotAChild,
    /// GNU's `p->alive == 1`.
    Unreaped(ReapableChild),
    /// GNU's `p->alive == 0`: the pid has gone back to the OS.
    ///
    /// It carries nothing, and that is the point -- there is no pid here for
    /// `kill(2)` or `waitpid(2)` to be given.  The terminal STATUS is not kept
    /// here either, because `Process::pending_status`/`Process::status` is
    /// where this port records it, exactly as GNU records it in
    /// `p->raw_status` rather than in `p->alive`.
    Reaped,
}

impl ChildOwnership {
    /// Take ownership of a freshly spawned `std::process::Child`.
    pub(crate) fn of_pipe_child(child: std::process::Child) -> Self {
        Self::Unreaped(ReapableChild {
            pid: child.id(),
            handle: ChildHandle::Pipe(Box::new(child)),
        })
    }

    /// Take ownership of a freshly spawned `portable_pty` child.
    ///
    /// A child `portable_pty` cannot name a pid for is [`Self::NotAChild`],
    /// because a pidless child is exactly what `eassert (child > 0)` rejects
    /// one frame below GNU's own sweep.
    pub(crate) fn of_pty_child(child: Box<dyn portable_pty::Child + Send + Sync>) -> Self {
        match child.process_id() {
            Some(pid) => Self::Unreaped(ReapableChild {
                pid,
                handle: ChildHandle::Pty(child),
            }),
            None => Self::NotAChild,
        }
    }

    /// Whether a child was ever spawned -- live or already reaped.
    ///
    /// This is *not* `p->alive`: a reaped child is still a child, and the
    /// `Real`-process predicates that ask "does this record have OS backing?"
    /// mean this question rather than the liveness one.
    pub(crate) fn has_child(&self) -> bool {
        !matches!(self, Self::NotAChild)
    }

    /// GNU's `p->alive`: the pid, and only while this port still owns it.
    ///
    /// Every `kill(2)` aimed at one of this port's own children goes through
    /// here, so `record_kill_process`'s `if (p->alive)` is not a test anyone
    /// has to remember to write.
    pub(crate) fn pid_if_unreaped(&self) -> Option<u32> {
        match self {
            Self::Unreaped(child) => Some(child.pid),
            Self::NotAChild | Self::Reaped => None,
        }
    }

    /// The spawn stored a `portable_pty` handle rather than a
    /// `std::process::Child`.
    pub(crate) fn is_pty_handle(&self) -> bool {
        matches!(
            self,
            Self::Unreaped(ReapableChild {
                handle: ChildHandle::Pty(_),
                ..
            })
        )
    }

    /// The child's stdin while it exists.  Not a reap, and the only reason the
    /// handle is reachable at all.
    pub(crate) fn stdin_mut(&mut self) -> Option<&mut std::process::ChildStdin> {
        match self {
            Self::Unreaped(ReapableChild {
                handle: ChildHandle::Pipe(child),
                ..
            }) => child.stdin.as_mut(),
            _ => None,
        }
    }

    /// The child's stdin for the poller registrations, which need the
    /// descriptor and nothing else.
    pub(crate) fn stdin(&self) -> Option<&std::process::ChildStdin> {
        match self {
            Self::Unreaped(ReapableChild {
                handle: ChildHandle::Pipe(child),
                ..
            }) => child.stdin.as_ref(),
            _ => None,
        }
    }

    /// Whether this process has a pipe child at all -- the `else if let
    /// Some(child) = ...` this port's write path used to spell against the
    /// handle itself.
    pub(crate) fn has_pipe_child(&self) -> bool {
        matches!(
            self,
            Self::Unreaped(ReapableChild {
                handle: ChildHandle::Pipe(_),
                ..
            })
        )
    }

    /// GNU's `process-send-eof` for a pipe child: close stdin.  Answers
    /// whether there was a pipe child to close it on.
    pub(crate) fn close_stdin(&mut self) -> bool {
        match self {
            Self::Unreaped(ReapableChild {
                handle: ChildHandle::Pipe(child),
                ..
            }) => {
                drop(child.stdin.take());
                true
            }
            _ => false,
        }
    }

    /// GNU's `child_status_changed (p->pid, &status, WUNTRACED | WCONTINUED)`
    /// (src/process.c:7742), and the state transition its terminal answers
    /// cause.
    ///
    /// [`ChildStatusChange::NoChange`] for a process that is not, or is no
    /// longer, this port's to wait for -- which is GNU's `p->alive &&` at
    /// :7741 rather than an early `continue` inside the loop.
    pub(crate) fn poll_status(&mut self) -> ChildStatusChange {
        let Self::Unreaped(child) = self else {
            return ChildStatusChange::NoChange;
        };
        let change = child.probe();
        if matches!(
            change,
            ChildStatusChange::Reaped(_) | ChildStatusChange::Gone
        ) {
            *self = Self::Reaped;
        }
        change
    }

    /// GNU's `record_kill_process` (src/callproc.c:196-211) followed by the
    /// blocking reap the delete path needs so the child does not become a
    /// zombie nothing revisits.
    ///
    /// The `if (p->alive)` is the `let ... else`, and there is no other arm: a
    /// reaped child is neither signalled nor waited a second time.
    pub(crate) fn terminate_and_reap(&mut self, kill_signal: i32) {
        let Self::Unreaped(child) = self else {
            return;
        };
        // One probe, through the owner, so an already-exited child is settled
        // rather than signalled.  GNU reaches the same place from the other
        // side: its handler has already cleared `p->alive`.
        if matches!(
            child.probe(),
            ChildStatusChange::Reaped(_) | ChildStatusChange::Gone
        ) {
            *self = Self::Reaped;
            return;
        }
        child.kill_group_then_handle(kill_signal);
        child.waitpid_blocking();
        *self = Self::Reaped;
    }

    /// GNU's `wait_for_termination (child, NULL, ...)` (src/sysdep.c:500,
    /// called that way at src/callproc.c:257) on a child that is already
    /// terminal or has just been signalled.
    pub(crate) fn wait_for_termination(&mut self) {
        let Self::Unreaped(child) = self else {
            return;
        };
        child.waitpid_blocking();
        *self = Self::Reaped;
    }

    /// The handle's own `kill`, for the callers that fall back to it when the
    /// process group would not take the signal.
    pub(crate) fn kill_handle(&mut self) {
        let Self::Unreaped(child) = self else {
            return;
        };
        child.kill_handle();
    }
}

impl ReapableChild {
    /// One non-blocking probe, decoded into the port's status vocabulary.
    ///
    /// The dispatch is the one `poll_child_status_change` already had, kept
    /// exactly: a `portable_pty` child is probed through its own handle, and
    /// everything else through the raw `waitpid` -- which is what carries
    /// `WUNTRACED | WCONTINUED`, so the stop and continue transitions GNU asks
    /// for at :7742 stay visible.
    fn probe(&mut self) -> ChildStatusChange {
        match &mut self.handle {
            ChildHandle::Pty(child) => match child.try_wait() {
                Ok(Some(status)) => {
                    ChildStatusChange::Reaped(super::process_status_from_pty_exit(&status))
                }
                Ok(None) => ChildStatusChange::NoChange,
                Err(_) => ChildStatusChange::Reaped(process_status_exit_value(1)),
            },
            ChildHandle::Pipe(child) => Self::probe_pipe(self.pid, child),
        }
    }

    #[cfg(unix)]
    fn probe_pipe(pid: u32, _child: &mut std::process::Child) -> ChildStatusChange {
        super::process_child_status_change_from_wait(super::sys::poll_child_status(pid))
    }

    #[cfg(not(unix))]
    fn probe_pipe(_pid: u32, child: &mut std::process::Child) -> ChildStatusChange {
        match child.try_wait() {
            Ok(Some(status)) => ChildStatusChange::Reaped(super::process_status_from_exit(&status)),
            Ok(None) => ChildStatusChange::NoChange,
            Err(_) => ChildStatusChange::Reaped(process_status_exit_value(1)),
        }
    }

    /// GNU's `wait_for_termination`: `get_child_status (child, status, 0,
    /// interruptible)` (src/sysdep.c:500-503).
    fn waitpid_blocking(&mut self) {
        match &mut self.handle {
            ChildHandle::Pipe(child) => {
                let _ = child.wait();
            }
            ChildHandle::Pty(child) => {
                let _ = child.wait();
            }
        }
    }

    /// GNU's `kill (- p->pid, SIGKILL)` (src/callproc.c:206), with this port's
    /// existing fallback to the handle's own kill when the group does not take
    /// it.
    fn kill_group_then_handle(&mut self, signal_num: i32) {
        if super::sys::send_signal_to_group(self.pid as i64, signal_num) == 0 {
            return;
        }
        self.kill_handle();
    }

    fn kill_handle(&mut self) {
        match &mut self.handle {
            ChildHandle::Pipe(child) => {
                let _ = child.kill();
            }
            ChildHandle::Pty(child) => {
                let _ = child.kill();
            }
        }
    }
}

impl std::fmt::Debug for ChildOwnership {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAChild => f.write_str("NotAChild"),
            Self::Unreaped(child) => write!(f, "Unreaped({})", child.pid),
            Self::Reaped => f.write_str("Reaped"),
        }
    }
}
