//! GNU's asynchronous child-status recording (`handle_child_signal`,
//! src/process.c:7691), and the type that keeps Lisp from seeing an exited
//! child as running.
//!
//! # What GNU does, and where
//!
//! `handle_child_signal` is the SIGCHLD handler.  It walks the process alist
//! itself and stamps every child whose state changed:
//!
//! ```c
//!   FOR_EACH_PROCESS (tail, proc)                          /* :7734 */
//!     {
//!       struct Lisp_Process *p = XPROCESS (proc);
//!       int status;
//!
//!       if (p->alive
//!           && child_status_changed (p->pid, &status,
//!                                    WUNTRACED | WCONTINUED))   /* :7741-7742 */
//!         {
//!           changed = true;
//!           p->tick = ++process_tick;                      /* :7745 */
//!           p->raw_status = status;                        /* :7746 */
//!           p->raw_status_new = 1;                         /* :7747 */
//!
//!           if (WIFSIGNALED (status) || WIFEXITED (status)) /* :7750 */
//!             {
//!               bool clear_desc_flag = 0;
//!               p->alive = 0;                              /* :7752 */
//!               if (p->infd >= 0) clear_desc_flag = 1;
//!               if (clear_desc_flag) delete_read_fd (p->infd);   /* :7760 */
//!             }
//!         }
//!     }
//!   if (changed) child_signal_notify ();                   /* :7766-7767 */
//! ```
//!
//! Its own header states the contract in one sentence (:7668-7671):
//!
//! ```text
//!    All we do is change the status; we do not run sentinels or print
//!    notifications.  That is saved for the next time keyboard input is
//!    done, in order to avoid timing errors.
//! ```
//!
//! So the recording is *only* a recording, and the notification is
//! `status_notify`'s job (:7862).  The consequence is Lisp-visible and is
//! what this module exists for: in GNU, `(process-live-p exited-child)` is
//! `nil` with nobody having called `accept-process-output` or waited at all.
//!
//! # Why the sweep is NOT in a signal handler here
//!
//! GNU's own comments above `handle_child_signal` enumerate what a SIGCHLD
//! handler may legitimately do, and the list is short (:7673-7688):
//!
//! * *"** WARNING: this can be called during garbage collection.  Therefore,
//!   it must not be fooled by the presence of mark bits in Lisp objects."*
//! * *"** Malloc WARNING: This should never call malloc either directly or
//!   indirectly; if it does, that is a bug."*
//!
//! and `child_signal_notify` (:7616-7650) carries the third, with a stack
//! trace as evidence: an `emacs_perror` was REMOVED from the handler because
//! `strerror_l` is not reentrant and reaches `malloc` through the locale
//! machinery.  All the handler is allowed to do at the end is
//! `emacs_write (fd, &dummy, 1)` to a self-pipe.
//!
//! Those three constraints are the whole design input, and in a Rust port
//! they are decisive.  This port's process table is a
//! `HashMap<ProcessId, Process>` owned by the Lisp thread; a signal is
//! delivered to an arbitrary thread (which is why GNU has
//! `deliver_process_signal`'s FORWARD_SIGNAL_TO_MAIN_THREAD, src/sysdep.c:
//! 1729-1751), and reading that map from a handler while the Lisp thread
//! mutates it is a data race, not merely a lock-order problem.  Iterating it
//! allocates.  So the sweep cannot live where GNU's lives.
//!
//! What CAN live in a handler is exactly what GNU puts there at the end: a
//! byte on a self-pipe.  That is a wake-up, and it changes no Lisp answer --
//! it only decides how soon a safe point is reached.  This port already has
//! the wake-up in another form: a `pidfd` per child, registered with the wait
//! poller (`sys::ChildStatusSource`), which makes the poller return the
//! moment a child terminates.
//!
//! **So the recording is placed where it is safe, and WHICH safe point is the
//! whole question.**  Ledger 193 chose `Context::maybe_quit` -- GNU's
//! `pending_signals` check -- and that was wrong.  Ledger 198 moved it to
//! GNU's own site and made the move a type; the reasoning is below and the
//! enforcement is
//! [`WaitStatusNotifySite`](crate::emacs_core::wait::WaitStatusNotifySite).
//! Separately, a subr still cannot report a status without naming where its
//! record came from: [`ObservedProcess`] has private fields, and the
//! constructor that observes takes an [`UpdateStatusSite`] naming which of
//! GNU's eight `update_status` lines the caller is.
//!
//! # Where GNU puts it, and why `maybe_quit` is not it
//!
//! GNU's own comment says the recording exists so that the answer is ready
//! *"the next time keyboard input is done"* (:7669-7671) -- and the function
//! that does keyboard input is `wait_reading_process_output`.  Reading the
//! call graph rather than the phrase says the same thing three ways:
//!
//! * `process_pending_signals`, which is all `maybe_quit` reaches
//!   (src/lisp.h:3896-3900 -> src/eval.c:1868-1876), is
//!   `pending_signals = false; handle_async_input (); do_pending_atimers ();`
//!   (src/keyboard.c:8367-8372).  `grep -c status_notify` over it is **0**.
//! * `handle_child_signal` never sets `pending_signals` at all: `grep -n
//!   'pending_signals = ' src/*.c` returns eleven lines and **not one is in
//!   `process.c`**.  Its wake is `child_signal_notify`, a byte on the
//!   self-pipe the `select` inside the wait is watching.
//! * All five `status_notify` calls are `Fdelete_process` (:1129, :1149),
//!   `process_send_signal`'s SIGCONT arm (:7181) -- three subrs notifying a
//!   status they wrote themselves on the line above -- and
//!   `wait_reading_process_output` (:5554, :5854).  **Every status GNU
//!   discovers asynchronously is notified from the wait.**
//!
//! GNU says why in its own words, at :3413 and again at :6160: *"Execute the
//! sentinel here.  If we had relied on status_notify to do it later, it will
//! read input from the process before calling the sentinel."*
//!
//! # What the wrong safe point cost, measured
//!
//! With the record made at `maybe_quit`, a status became Lisp-visible at a
//! moment when no wait was running, so `(while (process-live-p p)
//! (accept-process-output nil 0.02))` -- the commonest process idiom there is
//! -- could exit its loop with the sentinel unrun.  Programs depend on the
//! opposite: `magit-run-post-commit-hook` is keyed on `last-command`, which
//! its caller binds AROUND that loop.  100 runs per shape, `-Q --batch`,
//! counting only the runs in which the loop actually entered a wait:
//!
//! ```text
//!                              entered a wait   sentinel inside the let
//!   GNU 31.0.90                       294               294
//!   this port, drain at maybe_quit    300               261
//! ```
//!
//! and `treemacs_magit_package_batch`'s
//! `extending_a_real_commit_schedules_the_same_project_refresh` failed
//! deterministically with `(error "Treemacs-Magit idle update was not
//! scheduled")` -- which is exactly what ledger 180 measured when it declined
//! the synchronous sweep, and for the same reason.
//!
//! # What that costs, stated rather than buried
//!
//! GNU's handler *reaps* the child (`child_status_changed` -> `waitpid`), so
//! GNU answers these with nobody having waited at all, and this port does not:
//!
//! ```text
//!                                          GNU 31.0.90   here
//!   (process-status p) after a pure spin       exit       run
//!   (process-attributes pid) 'state            nil        "Z"
//!   (signal-process p 0)                       -1         0
//! ```
//!
//! Those are ledger 180 §9.1-9.2 and ledger 184's rows 2 and 3, and they are
//! PINNED divergences again.  The only safe point that closes them is GNU's
//! own -- the handler -- and a Rust port cannot walk this table there.  Ledger
//! 187 §8.1(b) asked for "a safe point that is not a Lisp observation"; the
//! answer is that GNU has none either.  GNU's safe point for child status IS a
//! Lisp observation: the wait.
//!
//! # The pipe is not a child, and cannot be swept
//!
//! `handle_child_signal` passes `p->pid` to `child_status_changed`, and
//! `get_child_status` opens with `eassert (child > 0)` (src/sysdep.c:461).
//! A pipe, network or serial connection has no pid, so the handler cannot
//! reach it; its status changes in exactly one other place --
//! `read_process_output` returning 0 (:6072-6079), which is inside the wait.
//! Ledger 165's finding is that `process-live-p` therefore means the
//! OPPOSITE thing for a pipe, and a fix keyed on "the child exited" that
//! touched pipes would be wrong for half the process kinds.
//!
//! [`SweepableChild`] is that rule as a type: it carries the OS pid, and its
//! only constructor is the membership test.  A pidless process is not
//! skipped by an `if` inside the loop -- it cannot be built, so it cannot be
//! in the population.

use super::reap::{ChildOwnershipIo, ReapableChildIo};
use super::{
    Process, ProcessId, ProcessManager, ProcessStatusSymbol, Value, process_effective_status,
    process_public_status_symbol,
};
use neovm_eval::emacs_core::eval::data::process_state::{StatusChangeSite, StatusChangeTicks};

/// A process that GNU's SIGCHLD sweep may harvest.
///
/// GNU's membership test is `p->alive && child_status_changed (p->pid, ...)`
/// (src/process.c:7741-7742), and the pid half of it is enforced one frame
/// down by `eassert (child > 0)` (src/sysdep.c:461).  Both halves are in
/// [`SweepableChild::of`], and there is no other constructor: a connection
/// with no child is not a member of the population rather than an early
/// `continue` inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct SweepableChild {
    id: ProcessId,
}

impl SweepableChild {
    /// GNU's `p->alive` (:7741) plus the pid that `get_child_status` requires.
    ///
    /// This port spells `p->alive` as "the recorded status is one that can
    /// still change" -- `run` or `stop`, exactly the pair
    /// `poll_child_status_change` already keeps polling so a later
    /// `WCONTINUED` stays observable -- and spells `p->pid` as any of the
    /// three child handles a spawn may have left behind.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn of(id: ProcessId, proc: &Process) -> Option<Self> {
        let status_can_change = matches!(
            ProcessStatusSymbol::from_status_value(proc.status),
            Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Stop)
        );
        // GNU's `p->alive` itself, since ledger 187: the pid is present in
        // exactly the state in which `waitpid` may be called on it.
        let alive = proc.live_io.child.pid_if_unreaped().is_some();
        (status_can_change && alive).then_some(Self { id })
    }

    pub(crate) fn id(self) -> ProcessId {
        self.id
    }
}

/// One of the eight places GNU calls `update_status` (src/process.c:717).
///
/// The list is closed and mechanically derivable -- `grep -n 'update_status
/// ('` over src/process.c gives the definition plus exactly these eight call
/// sites -- so it can be a finite type rather than a convention.  Every read
/// of a Lisp-visible process status in this port has to name the GNU line it
/// is, and [`UpdateStatusSite::recording`] then decides whether that line
/// needs the sweep run first.
///
/// The point of the enum is the same as ledger 177's `PostImageInit`: a new
/// site cannot be added without a GNU citation and a classification, because
/// [`UpdateStatusSite::ALL`] is declared with length
/// [`UpdateStatusSite::COUNT`] (derived from the last discriminant) and
/// [`UpdateStatusSite::gnu`] and [`UpdateStatusSite::recording`] are
/// exhaustive matches.  An empty or short table is a compile error, not a
/// silent omission, and `child_status_test.rs` asserts the absolute count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum UpdateStatusSite {
    /// `Fdelete_process` (:1143).  This one does NOT harvest, and the line
    /// above it is why: `p->raw_status_new = 0;` at :1123 THROWS THE RECORD
    /// AWAY before anything else happens, so :1141's `if (p->raw_status_new)`
    /// can only be true for a status that `record_kill_process`'s own SIGKILL
    /// produced between :1136 and :1140.  Measured, a child that exited 7
    /// with nobody waiting: both editors answer `signal` / 9, because GNU
    /// discards the 7 and this port never recorded it.
    #[allow(dead_code)] // GNU's line, in the table for parity; only `ALL` names it
    DeleteProcess = 0,
    /// `Fprocess_status` (:1189).  Also `process-live-p`, which is
    /// `(memq (process-status process) '(run open listen connect stop))`
    /// in lisp/subr.el:3538-3540 and has no C of its own.
    ProcessStatus,
    /// `Fprocess_exit_status` (:1213).
    ProcessExitStatus,
    /// `send_process` (:6726) -- `process-send-string` and
    /// `process-send-region`, which error "Process %s not running: %s" when
    /// the settled status is not `run` (:6727-6728).
    SendProcess,
    /// `Fprocess_send_eof` (:7453), with the same "not running" error
    /// (:7454-7455).
    ProcessSendEof,
    /// `wait_reading_process_output` (:5562), on the process being waited
    /// for.
    #[allow(dead_code)] // GNU's line, in the table for parity; only `ALL` names it
    WaitReadingProcessOutput,
    /// `read_process_output`'s pipe-connection EOF arm (:6087).
    #[allow(dead_code)] // GNU's line, in the table for parity; only `ALL` names it
    ReadProcessOutputPipeEof,
    /// `status_notify` (:7915), immediately before `status_message` and the
    /// removal decision.
    #[allow(dead_code)] // GNU's line, in the table for parity; only `ALL` names it
    StatusNotify,
}

/// Where the record this site reads was made.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum Recording {
    /// GNU reaches this site with the record already made *and so does this
    /// port*, because the site is inside the wait/notification machinery
    /// that has just done the discovery itself.
    AlreadyRecorded {
        /// Where this port made the record, so the claim is checkable.
        #[allow(dead_code)] // a citation: read only by the table test in process/tests
        by: &'static str,
    },
    /// GNU reaches this site with the record already made because
    /// `handle_child_signal` ran ASYNCHRONOUSLY.  This port makes it
    /// asynchronously too -- from the SIGCHLD trigger -- but only inside
    /// `wait_reading_process_output`, so a program that never waits reads the
    /// stale status here, which is the pinned divergence the module docs give
    /// the three rows for.
    ///
    /// **Nothing sweeps HERE, and that is the whole point.**  Sweeping at the
    /// observation -- running GNU's `handle_child_signal` body on demand --
    /// gives the right answer to the question and the wrong answer to the
    /// program: GNU's record is late by the time a SIGCHLD takes to be
    /// delivered and handled, and a `waitpid (WNOHANG)` at the observation is
    /// ground truth.  Ledger 180 measured what that costs -- `(while
    /// (process-live-p p) (accept-process-output p 1))` losing its sentinel
    /// 4/60, and `treemacs-magit`'s
    /// `extending_a_real_commit_schedules_the_same_project_refresh` failing
    /// DETERMINISTICALLY, because magit's post-commit hook is keyed on
    /// `last-command` and the sentinel then runs after the `let` that bound
    /// it has unwound -- and withdrew the wiring for it.
    ///
    /// Ledger 193's trigger was declared to avoid that by being LATE rather
    /// than synchronous, and the reasoning was wrong: lateness is not the
    /// property that matters.  What matters is whether the record is published
    /// with its sentinel in the same call.  A record made at `maybe_quit` is
    /// late AND unnotified, so it reproduced 180's failure exactly -- see the
    /// module docs for the 294/294 against 261/300.
    AsynchronouslyRecorded {
        /// GNU's line that makes the record for this site.
        #[allow(dead_code)] // a citation: read only by the table test in process/tests
        by: &'static str,
        /// Where this port makes it, so the claim is checkable.
        #[allow(dead_code)] // a citation: read only by the table test in process/tests
        here: &'static str,
    },
}

impl UpdateStatusSite {
    /// Derived from the last discriminant, so a new variant that is not added
    /// to [`Self::ALL`] is a compile error.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const COUNT: usize = Self::StatusNotify as usize + 1;

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::DeleteProcess,
        Self::ProcessStatus,
        Self::ProcessExitStatus,
        Self::SendProcess,
        Self::ProcessSendEof,
        Self::WaitReadingProcessOutput,
        Self::ReadProcessOutputPipeEof,
        Self::StatusNotify,
    ];

    /// `file:line` of the `update_status` call in the GNU tree.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn gnu(self) -> &'static str {
        match self {
            Self::DeleteProcess => "src/process.c:1143",
            Self::ProcessStatus => "src/process.c:1189",
            Self::ProcessExitStatus => "src/process.c:1213",
            Self::SendProcess => "src/process.c:6726",
            Self::ProcessSendEof => "src/process.c:7453",
            Self::WaitReadingProcessOutput => "src/process.c:5562",
            Self::ReadProcessOutputPipeEof => "src/process.c:6087",
            Self::StatusNotify => "src/process.c:7915",
        }
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn recording(self) -> Recording {
        match self {
            // The four Lisp entry points a program can reach with no wait
            // having run at all.  These were the divergence until ledger 193.
            Self::ProcessStatus
            | Self::ProcessExitStatus
            | Self::SendProcess
            | Self::ProcessSendEof => Recording::AsynchronouslyRecorded {
                by: "handle_child_signal, src/process.c:7734-7763",
                here: "os_signal::HandledSignal::Sigchld -> drain_and_notify_child_statuses \
                       -> record_child_status_changes, inside \
                       wait_reading_process_output (GNU src/process.c:5554, :5854)",
            },
            // The four sites that must not, or need not, sweep here.
            Self::DeleteProcess => Recording::AlreadyRecorded {
                by: "src/process.c:1123 discards the record before this line reads it",
            },
            Self::WaitReadingProcessOutput => Recording::AlreadyRecorded {
                by: "service_process_output_for_ids -> check_child_status_change",
            },
            Self::ReadProcessOutputPipeEof => Recording::AlreadyRecorded {
                by: "the pipe EOF arm has no child to sweep (SweepableChild::of)",
            },
            Self::StatusNotify => Recording::AlreadyRecorded {
                by: "run_process_status_notification is entered on status_notify_pending",
            },
        }
    }
}

/// A process whose child status has been recorded, and the only route by
/// which a *subr* can obtain one.
///
/// [`process_effective_status`] (GNU `update_status`'s view, src/process.c:
/// 717-721) and [`process_public_status_symbol`] (GNU `Fprocess_status`'s
/// return value, :1188-1201) were `pub(crate)`; they are now private to the
/// parent module and to this child of it, so no `builtins` entry point can
/// call either.  The two methods below are their only public spelling, and
/// an `ObservedProcess` has private fields and exactly two constructors:
/// [`ProcessManager::observe`], which sweeps for every site whose
/// [`Recording`] says so, and
/// [`ProcessManager::read_status_without_recording`], which takes an
/// enumerated [`UnrecordedStatusRead`].
///
/// **The scope of that guarantee, stated exactly, because it is narrower
/// than "nothing can read a status".**  It covers the Lisp-visible ANSWER:
/// to write a subr that reports a process's status, you must name a GNU
/// `update_status` line or one of the enumerated holes.  It does not cover
/// the manager's own internals, which read the `status` field directly and
/// must -- 24 such reads, of which the seven that reach a Lisp answer all go
/// through a named funnel (`gnu_process_status_message_for_status` for the
/// sentinel text, `process_status_ends_target_wait` for the wait, the
/// `live_process_ids` predicates for the service order, and
/// `internal-default-process-sentinel`, `delete-process` and
/// `continue-process`, none of which GNU passes through `update_status`
/// either: GNU's own `Finternal_default_process_sentinel` reads `p->status`
/// bare at :7958, and `Fcontinue_process` never touches it).
///
/// Within that scope the point stands: "the child has exited and Lisp still
/// reads `run`" is not rejected by a check, it is a sentence with no
/// grammar -- to write it you would need a status value, and a subr's status
/// values all come from here.
pub(crate) struct ObservedProcess<'a> {
    proc: &'a Process,
}

impl<'a> ObservedProcess<'a> {
    /// Private: the only caller is [`ProcessManager::observe`], in this
    /// module, after the sweep.
    pub(crate) fn new(proc: &'a Process) -> Self {
        Self { proc }
    }

    /// GNU `p->status` after `update_status` (:717-721): the raw pair, e.g.
    /// `(exit . 7)`, as `Fprocess_exit_status` reads it (:1214-1218).
    pub(crate) fn settled_status(&self) -> Value {
        process_effective_status(self.proc)
    }

    /// GNU `Fprocess_status`'s return value, after the connection remap of
    /// :1193-1201.
    pub(crate) fn public_status_symbol(&self) -> Value {
        process_public_status_symbol(self.proc)
    }

    /// GNU `send_process`'s liveness gate (src/process.c:6725-6728) and
    /// `Fprocess_send_eof`'s (:7451-7455), which are the same two lines:
    /// `update_status`, then `! EQ (p->status, Qrun)` is an error.  GNU reads
    /// `p->status` there because `update_status` has just WRITTEN it; this
    /// port reads the settled view instead, which is the same value.
    pub(crate) fn allows_send(&self) -> bool {
        super::process_allows_send(self.proc)
    }

    /// The process itself, for the fields that are not its status.
    pub(crate) fn process(&self) -> &'a Process {
        self.proc
    }
}

#[cfg(not(feature = "test-support"))]
pub(crate) use neovm_eval::emacs_core::eval::data::process_state::UnrecordedStatusRead;
#[cfg(feature = "test-support")]
pub use neovm_eval::emacs_core::eval::data::process_state::UnrecordedStatusRead;

// ---------------------------------------------------------------------------
// GNU's per-process tick pair
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Engagement counters
// ---------------------------------------------------------------------------

/// How often the walk ran, and how many processes it stamped.
///
/// **Engagement counters, not telemetry.**  Ledger P5.2's skip was 100% green
/// and fired ZERO times, so a mechanism that can silently never run has to be
/// able to say how often it ran -- and `tracing` cannot answer it, because a
/// release `--batch` run emits no `debug` records at all.
///
/// The walk is unconditional since ledger 208, so `walks` is a rate rather
/// than an arming count; `stamped` is what it found.  The third number lives
/// next door -- [`STATUS_NOTIFY_VISITED`] -- and the difference `visited -
/// stamped` is exactly the work the old visit set (the walk's own return
/// value) could not have done.
pub(crate) static STATUS_NOTIFY_WALKS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static STATUS_NOTIFY_STAMPED: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
/// Processes GNU's `p->tick != p->update_tick` (src/process.c:7892) put in a
/// `status_notify` visit set.  See [`STATUS_NOTIFY_WALKS`].
static STATUS_NOTIFY_VISITED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `(walks, stamped, visited)` since the process started.  See the statics
/// above.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn status_notify_totals() -> (u64, u64, u64) {
    use std::sync::atomic::Ordering;
    (
        STATUS_NOTIFY_WALKS.load(Ordering::Relaxed),
        STATUS_NOTIFY_STAMPED.load(Ordering::Relaxed),
        STATUS_NOTIFY_VISITED.load(Ordering::Relaxed),
    )
}

pub(crate) fn record_status_notify_visits(count: usize) {
    STATUS_NOTIFY_VISITED.fetch_add(count as u64, std::sync::atomic::Ordering::Relaxed);
}
