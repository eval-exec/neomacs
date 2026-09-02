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

use super::{
    Process, ProcessId, ProcessManager, ProcessStatusSymbol, Value, process_effective_status,
    process_public_status_symbol,
};

/// A process that GNU's SIGCHLD sweep may harvest.
///
/// GNU's membership test is `p->alive && child_status_changed (p->pid, ...)`
/// (src/process.c:7741-7742), and the pid half of it is enforced one frame
/// down by `eassert (child > 0)` (src/sysdep.c:461).  Both halves are in
/// [`SweepableChild::of`], and there is no other constructor: a connection
/// with no child is not a member of the population rather than an early
/// `continue` inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SweepableChild {
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
    pub(super) fn of(id: ProcessId, proc: &Process) -> Option<Self> {
        let status_can_change = matches!(
            ProcessStatusSymbol::from_status_value(proc.status),
            Some(ProcessStatusSymbol::Run | ProcessStatusSymbol::Stop)
        );
        // GNU's `p->alive` itself, since ledger 187: the pid is present in
        // exactly the state in which `waitpid` may be called on it.
        let alive = proc.live_io.child.pid_if_unreaped().is_some();
        (status_can_change && alive).then_some(Self { id })
    }

    pub(super) fn id(self) -> ProcessId {
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
pub(crate) enum UpdateStatusSite {
    /// `Fdelete_process` (:1143).  This one does NOT harvest, and the line
    /// above it is why: `p->raw_status_new = 0;` at :1123 THROWS THE RECORD
    /// AWAY before anything else happens, so :1141's `if (p->raw_status_new)`
    /// can only be true for a status that `record_kill_process`'s own SIGKILL
    /// produced between :1136 and :1140.  Measured, a child that exited 7
    /// with nobody waiting: both editors answer `signal` / 9, because GNU
    /// discards the 7 and this port never recorded it.
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
    WaitReadingProcessOutput,
    /// `read_process_output`'s pipe-connection EOF arm (:6087).
    ReadProcessOutputPipeEof,
    /// `status_notify` (:7915), immediately before `status_message` and the
    /// removal decision.
    StatusNotify,
}

/// Where the record this site reads was made.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Recording {
    /// GNU reaches this site with the record already made *and so does this
    /// port*, because the site is inside the wait/notification machinery
    /// that has just done the discovery itself.
    AlreadyRecorded {
        /// Where this port made the record, so the claim is checkable.
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
        by: &'static str,
        /// Where this port makes it, so the claim is checkable.
        here: &'static str,
    },
}

impl UpdateStatusSite {
    /// Derived from the last discriminant, so a new variant that is not added
    /// to [`Self::ALL`] is a compile error.
    pub(crate) const COUNT: usize = Self::StatusNotify as usize + 1;

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

    /// The Lisp entry point, or `""` for a site with no Lisp name of its own.
    pub(crate) fn lisp(self) -> &'static str {
        match self {
            Self::DeleteProcess => "delete-process",
            Self::ProcessStatus => "process-status",
            Self::ProcessExitStatus => "process-exit-status",
            Self::SendProcess => "process-send-string",
            Self::ProcessSendEof => "process-send-eof",
            Self::WaitReadingProcessOutput => "accept-process-output",
            Self::ReadProcessOutputPipeEof => "",
            Self::StatusNotify => "",
        }
    }

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
                by: "poll_process_output_for_ids -> check_child_status_change",
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
    fn new(proc: &'a Process) -> Self {
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

impl ProcessManager {
    /// GNU `handle_child_signal`'s `FOR_EACH_PROCESS` arm (src/process.c:
    /// 7734-7763), run at a safe point instead of in the handler.
    ///
    /// The walk order is GNU's: `FOR_EACH_PROCESS` is
    /// `FOR_EACH_ALIST_VALUE (Vprocess_alist, ...)` (:343) and `make_process`
    /// conses onto the front (:953), so the alist is newest-first and a
    /// descending `ProcessId` reproduces it -- the same identity
    /// `list_processes` and `live_process_ids` already use (ledger 175 §3).
    /// Order does not change what is recorded, since each child is harvested
    /// independently; it is matched so that a future reader does not have to
    /// wonder.
    ///
    /// `check_child_status_change` is GNU's per-process body: the
    /// `child_status_changed` probe (:7742), the `delete_read_fd` on a
    /// terminal status (:7760, spelled here as unregistering the child's
    /// status source from the poller), and the `raw_status`/`raw_status_new`
    /// stamp (:7746-7747, spelled `pending_status`/`status_notify_pending`).
    /// The `site` argument is the whole point, and it is not used at run time.
    /// See [`WaitStatusNotifySite`](crate::emacs_core::wait::WaitStatusNotifySite):
    /// it has no public constructor, so this walk is reachable only from
    /// `wait.rs` -- `Context::maybe_quit` cannot spell the call.
    ///
    /// Returns only a COUNT, for the engagement counters.  **Which processes
    /// `status_notify` then visits is not this walk's answer to give**: it is
    /// GNU's per-process tick pair, read by
    /// [`ProcessManager::processes_with_unnotified_status_change`], because
    /// seven of GNU's eight `p->tick = ++process_tick;` sites are not this walk
    /// (see [`StatusChangeSite`]).  Returning the stamped ids is what made the
    /// visit set the SIGCHLD record's, which is the defect this shape closes.
    pub(crate) fn record_child_status_changes(
        &mut self,
        site: crate::emacs_core::wait::WaitStatusNotifySite,
    ) -> usize {
        let _ = site;
        STATUS_NOTIFY_WALKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut population: Vec<SweepableChild> = self
            .processes
            .iter()
            .filter_map(|(id, proc)| SweepableChild::of(*id, proc))
            .collect();
        if population.is_empty() {
            return 0;
        }
        population.sort_unstable_by(|a, b| b.id().cmp(&a.id()));
        let mut stamped = 0;
        for child in population {
            if self.check_child_status_change(child.id()) {
                stamped += 1;
            }
        }
        STATUS_NOTIFY_STAMPED.fetch_add(stamped as u64, std::sync::atomic::Ordering::Relaxed);
        stamped
    }

    /// GNU's `update_status` at `site`, then the read.
    ///
    /// Returns `None` for an id that names no process at all, live or
    /// retired -- the analogue of `get_process` having answered `nil` before
    /// any of these sites was reached.
    pub(crate) fn observe(
        &mut self,
        site: UpdateStatusSite,
        id: ProcessId,
    ) -> Option<ObservedProcess<'_>> {
        match site.recording() {
            // NO ARM SWEEPS, and both variants say why rather than leaving it
            // to be inferred.  `AlreadyRecorded` needs nothing because the
            // machinery around the site has just made the record;
            // `AsynchronouslyRecorded` needs nothing because the SIGCHLD
            // trigger made it at a safe point -- and must not sweep here,
            // because a sweep AT the observation is what ledger 180 measured
            // and withdrew.  This `match` exists to make that a decision the
            // type forces rather than an omission.
            Recording::AlreadyRecorded { .. } | Recording::AsynchronouslyRecorded { .. } => {}
        }
        self.get_any(id).map(ObservedProcess::new)
    }
}

/// A Lisp-visible status read that this port CANNOT put the sweep in front
/// of, with the reason.
///
/// Every such read is a hole in the guarantee above, so the holes are a
/// finite type rather than a habit: adding one is adding a variant, which
/// forces a GNU citation and a written reason through the exhaustive
/// [`UnrecordedStatusRead::why`] match, and `COUNT` is asserted in
/// `process_test.rs` so a second hole cannot appear unremarked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum UnrecordedStatusRead {
    /// The `%s` mode-line construct.  GNU's `decode_mode_spec` spells it
    /// `obj = Fsymbol_name (Fprocess_status (obj));` (src/xdisp.c:29717-
    /// 29725), so in GNU it IS one of `Fprocess_status`'s callers and does
    /// harvest.
    ///
    /// Here it cannot: `expand_mode_line_percent_in_state` (xdisp.rs:2644)
    /// takes `&ProcessManager`, and so does every frame of the recursive
    /// mode-line renderer above it.  Threading `&mut` through redisplay to
    /// reach one `%` spec is a change to redisplay, not to process status,
    /// and it is not measurable from `--batch`: `format-mode-line` answers
    /// `""` for EVERY spec there, `%b` included, in both editors.
    ModeLinePercentS = 0,
}

impl UnrecordedStatusRead {
    pub(crate) const COUNT: usize = Self::ModeLinePercentS as usize + 1;
    pub(crate) const ALL: [Self; Self::COUNT] = [Self::ModeLinePercentS];

    pub(crate) fn gnu(self) -> &'static str {
        match self {
            Self::ModeLinePercentS => "src/xdisp.c:29723",
        }
    }

    pub(crate) fn why(self) -> &'static str {
        match self {
            Self::ModeLinePercentS => {
                "the recursive mode-line renderer holds &ProcessManager, not &mut"
            }
        }
    }
}

impl ProcessManager {
    /// Read a Lisp-visible status at one of the enumerated holes, WITHOUT
    /// GNU's recording having been made here.
    ///
    /// The `site` argument is not used at run time; it exists so the call
    /// cannot be written without naming which hole it is.
    pub(crate) fn read_status_without_recording(
        &self,
        site: UnrecordedStatusRead,
        id: ProcessId,
    ) -> Option<ObservedProcess<'_>> {
        let _ = site;
        self.get_any(id).map(ObservedProcess::new)
    }
}

// ---------------------------------------------------------------------------
// GNU's per-process tick pair
// ---------------------------------------------------------------------------

/// GNU's `p->tick = ++process_tick;` -- the assignment that puts a process
/// into `status_notify`'s visit set -- with **one variant per line in
/// `src/process.c` that spells it**.
///
/// `grep -n 'tick = ++process_tick' src/process.c` returns exactly eight lines
/// on GNU master since e381cf1fc97 (2025-08-15, "Allow child processes to
/// continue after EPIPE"), and they are the eight below.  The emacs-31.1
/// release (a360712c9d, 2026-08-24) predates that change on its branch and
/// still has a NINTH: `send_process`'s EPIPE arm at :6927, which synthesizes
/// `(exit . 256)` -- behavior 4d7e6e51dd4 introduced in 2012 and master no
/// longer has.  This port follows master's arm (see `write_process_input_once`),
/// so the ninth line is deliberately not a variant; the line numbers cited
/// on the eight variants are emacs-31.1's (master's are 1169, 1189, 6075,
/// 6092, 6101, 6158, 7193, 7752).  **Seven of them have nothing to do with
/// SIGCHLD**, which is the fact this type exists to keep in front of the next
/// reader: the record that decides whom `status_notify` visits is not the
/// child-signal record.  GNU declares the counter and the reason for it at
/// :232-235:
///
/// ```c
/// /* Number of events of change of status of a process.  */
/// static EMACS_INT process_tick;
/// /* Number of events for which the user or sentinel has been notified.  */
/// static EMACS_INT update_tick;
/// ```
///
/// A ninth site cannot be added without a GNU citation, because
/// [`Self::gnu`], [`Self::what`] and [`Self::notifier`] are exhaustive matches
/// and [`Self::COUNT`] (derived from the last discriminant) is asserted in
/// `process_test.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum StatusChangeSite {
    /// `Fdelete_process`'s network/serial/pipe arm (:1128).
    DeleteProcessConnection = 0,
    /// `Fdelete_process`'s real-subprocess arm, `infd >= 0` (:1148).
    DeleteProcessChild,
    /// `read_process_output`'s PTY `EIO` arm for a `pid == -2` process
    /// (:6058), which becomes `Qfailed`.
    ///
    /// **Never constructed outside the table, and the compiler is right about
    /// that.**  It is GNU's line all the same, and the table is the eight lines
    /// rather than the seven this port reaches; `Self::recorder`'s `NoAnalogue`
    /// arm says why there is nothing to record here, and deleting the `allow`
    /// is how a future entry that grows the window announces itself.
    #[allow(dead_code)]
    PtyEioBeforeFork,
    /// `wait_reading_process_output`'s pipe-connection EOF arm (:6075).
    PipeConnectionReadEof,
    /// `wait_reading_process_output`'s non-EOF read failure arm (:6084),
    /// which becomes `(exit . 256)`.
    SubprocessReadFailure,
    /// `connect_network_socket`'s failed non-blocking connect (:6141), which
    /// becomes `(failed . ERRNO)`.
    NonBlockingConnectFailed,
    /// `process_send_signal` sent `SIGCONT` (:7178), which becomes `Qrun`.
    ProcessSendSignalSigcont,
    /// `handle_child_signal`'s `child_status_changed` (:7746) -- the only one
    /// of the eight that is the SIGCHLD record.
    HandleChildSignal,
}

/// Who runs the sentinel for a change recorded at a [`StatusChangeSite`].
///
/// GNU's eight bump sites are not one mechanism.  Four of them call
/// `status_notify` on the very next lines, so the tick they just moved is
/// consumed immediately and no later walk ever sees it; the rest leave the
/// tick standing for the wait's own `status_notify` (:5554, :5854) to find.
///
/// The distinction is recorded here because it is what decides whether this
/// port needs the tick at a site at all, and getting it wrong in either
/// direction is a bug you can name: a site that notifies synchronously and
/// also leaves the tick standing runs its sentinel TWICE, and a site that
/// leaves the tick standing without anyone to consume it is visited by every
/// later walk forever.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusChangeNotifier {
    /// GNU calls `status_notify` within a few lines of the bump, so the tick
    /// is consumed in the same call.
    SynchronouslyAtTheSite {
        /// GNU's `status_notify` line for this site.
        gnu: &'static str,
        /// Where this port runs it, so the claim is checkable.
        here: &'static str,
    },
    /// GNU leaves the bump for the next `status_notify` the wait runs
    /// (:5554, :5854).
    TheWaitsStatusNotify,
}

/// Where THIS port makes the record for a [`StatusChangeSite`], or why it does
/// not.
///
/// A site with no analogue here is a hole, and a hole that is a variant cannot
/// be forgotten: [`StatusChangeSite::recorder`] is an exhaustive match, so
/// closing one is deleting a `NoAnalogue` arm and adding a citation.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusChangeRecorder {
    /// The function in this crate that calls
    /// [`ProcessManager::record_status_change`] for this site.
    Here(&'static str),
    /// GNU's state has no analogue here, with the reason.
    NoAnalogue { why: &'static str },
}

impl StatusChangeSite {
    /// Derived from the last discriminant, so a variant missing from
    /// [`Self::ALL`] is a compile error rather than a silent omission.
    #[cfg(test)]
    pub(crate) const COUNT: usize = Self::HandleChildSignal as usize + 1;

    #[cfg(test)]
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::DeleteProcessConnection,
        Self::DeleteProcessChild,
        Self::PtyEioBeforeFork,
        Self::PipeConnectionReadEof,
        Self::SubprocessReadFailure,
        Self::NonBlockingConnectFailed,
        Self::ProcessSendSignalSigcont,
        Self::HandleChildSignal,
    ];

    /// `file:line` of the `p->tick = ++process_tick;` in the GNU tree.
    pub(crate) fn gnu(self) -> &'static str {
        match self {
            Self::DeleteProcessConnection => "src/process.c:1128",
            Self::DeleteProcessChild => "src/process.c:1148",
            Self::PtyEioBeforeFork => "src/process.c:6058",
            Self::PipeConnectionReadEof => "src/process.c:6075",
            Self::SubprocessReadFailure => "src/process.c:6084",
            Self::NonBlockingConnectFailed => "src/process.c:6141",
            Self::ProcessSendSignalSigcont => "src/process.c:7178",
            Self::HandleChildSignal => "src/process.c:7746",
        }
    }

    /// The status GNU publishes at the site, so the table reads as a table.
    pub(crate) fn what(self) -> &'static str {
        match self {
            Self::DeleteProcessConnection => "(exit . 0) on a deleted connection",
            Self::DeleteProcessChild => "(signal . SIGKILL) on a deleted subprocess",
            Self::PtyEioBeforeFork => "failed, on EIO before the fork completed",
            Self::PipeConnectionReadEof => "(exit . 0) on a pipe connection's EOF",
            Self::SubprocessReadFailure => "(exit . 256) on a subprocess read failure",
            Self::NonBlockingConnectFailed => "(failed . ERRNO) on a failed connect",
            Self::ProcessSendSignalSigcont => "run, on SIGCONT",
            Self::HandleChildSignal => "the raw wait status of a changed child",
        }
    }

    /// Where this port records the change, or why it has nothing to record.
    #[cfg(test)]
    pub(crate) fn recorder(self) -> StatusChangeRecorder {
        match self {
            Self::DeleteProcessConnection | Self::DeleteProcessChild => {
                StatusChangeRecorder::Here("ProcessManager::stamp_process_for_delete")
            }
            Self::PipeConnectionReadEof => {
                StatusChangeRecorder::Here("ProcessManager::retire_pipe_process_at_read_eof")
            }
            Self::SubprocessReadFailure => {
                StatusChangeRecorder::Here("ProcessManager::retire_process_at_read_failure")
            }
            Self::NonBlockingConnectFailed => {
                StatusChangeRecorder::Here("ProcessManager::complete_pending_network_connect")
            }
            Self::ProcessSendSignalSigcont => {
                StatusChangeRecorder::Here("builtin_continue_process_impl")
            }
            Self::HandleChildSignal => {
                StatusChangeRecorder::Here("ProcessManager::set_child_status_pending")
            }
            // GNU's `p->pid == -2` is the window between `allocate_pty` and a
            // successful `fork` (src/process.c:6053-6060): the PTY master is
            // open and no child owns the slave yet, so an `EIO` on it means
            // the fork will never be reported by SIGCHLD.  This port has no
            // such window -- `spawn_child_with_environment` either returns a
            // child handle or reports the failure as a status in the same
            // call, so a process is never listed with a PTY and no child --
            // and `Fprocess_id` therefore has no `-2` to answer.
            Self::PtyEioBeforeFork => StatusChangeRecorder::NoAnalogue {
                why: "no pid == -2 window: the spawn publishes its own failure status",
            },
        }
    }

    /// Whether GNU consumes the bump on the spot or leaves it for the wait.
    #[cfg(test)]
    pub(crate) fn notifier(self) -> StatusChangeNotifier {
        match self {
            Self::DeleteProcessConnection => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:1129, status_notify (p, NULL)",
                here: "Context::delete_process_running_its_sentinel",
            },
            Self::DeleteProcessChild => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:1149, status_notify (p, NULL)",
                here: "Context::delete_process_running_its_sentinel",
            },
            Self::NonBlockingConnectFailed => StatusChangeNotifier::SynchronouslyAtTheSite {
                // GNU has no `status_notify` here; its :6141 bump is picked up
                // by the wait it is already inside.  This port completes a
                // pending connect and runs the `failed` sentinel in the same
                // service pass, which is the same call, so the tick must be
                // consumed there or the sentinel runs twice.
                gnu: "src/process.c:6141, inside wait_reading_process_output's own pass",
                here: "Context::poll_process_output_for_ids, \
                       PendingNetworkConnectCompletion::Failed",
            },
            Self::ProcessSendSignalSigcont => StatusChangeNotifier::SynchronouslyAtTheSite {
                gnu: "src/process.c:7181, status_notify (NULL, NULL)",
                here: "Context::builtin_continue_process -> notify_process_status_sentinel",
            },
            Self::PtyEioBeforeFork
            | Self::PipeConnectionReadEof
            | Self::SubprocessReadFailure
            | Self::HandleChildSignal => StatusChangeNotifier::TheWaitsStatusNotify,
        }
    }
}

/// GNU's `p->tick` / `p->update_tick` pair (src/process.h:144-147):
///
/// ```c
///     /* Event-count of last event in which this process changed status.  */
///     EMACS_INT tick;
///     /* Event-count of last such event reported.  */
///     EMACS_INT update_tick;
/// ```
///
/// It is one value rather than two fields because the only assignment GNU ever
/// makes to `update_tick` is `p->update_tick = p->tick;` -- at :7894 and again
/// at :7935 -- so [`Self::notified`] takes no argument and *"notified a tick
/// this process never reached"* is not a state that can be written.
///
/// This is deliberately NOT the same bit as `status_notify_pending`, which is
/// GNU's `raw_status_new`: GNU keeps them apart and sets them independently
/// (`Fdelete_process` at :1123-1128 and `process_send_signal` at :7176-7178
/// both CLEAR `raw_status_new` and MOVE the tick in the same breath).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatusChangeTicks {
    /// GNU `p->tick`.
    changed: u64,
    /// GNU `p->update_tick`.
    notified: u64,
}

impl StatusChangeTicks {
    /// GNU `p->tick = ++process_tick;`
    fn record(&mut self, tick: u64) {
        self.changed = tick;
    }

    /// GNU `p->update_tick = p->tick;` (:7894, and again at :7935).
    fn mark_notified(&mut self) {
        self.notified = self.changed;
    }

    /// GNU `p->tick != p->update_tick` (:7892) -- `status_notify`'s membership
    /// test, and the only question this type answers.
    pub(crate) fn is_unnotified(self) -> bool {
        self.changed != self.notified
    }
}

impl ProcessManager {
    /// GNU `p->tick = ++process_tick;` at `site`.
    ///
    /// The `site` argument is not used at run time; it exists so the call
    /// cannot be written without naming which of GNU's eight lines it is.
    pub(crate) fn record_status_change(&mut self, site: StatusChangeSite, id: ProcessId) {
        // GNU's `++process_tick` is a plain `EMACS_INT` increment.  `u64`
        // saturates rather than wraps so that a wrap could never make a
        // recorded change compare equal to a notified one; at one tick per
        // status change the bound is not reachable, and saying so is cheaper
        // than reasoning about it later.
        self.process_tick = self.process_tick.saturating_add(1);
        let tick = self.process_tick;
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status_ticks.record(tick);
        }
        tracing::trace!(
            gnu = site.gnu(),
            what = site.what(),
            tick,
            "p->tick = ++process_tick"
        );
    }

    /// GNU `p->update_tick = p->tick;` (src/process.c:7894 and :7935).
    pub(crate) fn mark_status_change_notified(&mut self, id: ProcessId) {
        if let Some(proc) = self.processes.get_mut(&id) {
            proc.status_ticks.mark_notified();
        }
    }

    /// GNU `status_notify`'s visit set: `FOR_EACH_PROCESS` filtered by
    /// `p->tick != p->update_tick` (src/process.c:7887-7892).
    ///
    /// The order is GNU's alist order -- `FOR_EACH_PROCESS` is
    /// `FOR_EACH_ALIST_VALUE (Vprocess_alist, ...)` (:343) and `make_process`
    /// conses onto the front (:953), so the list is newest-first and a
    /// descending [`ProcessId`] reproduces it.  That order is Lisp-visible for
    /// a split `:stderr` pipe, which is created BEFORE the process that owns
    /// it and therefore notified AFTER it (ledger 54).
    pub(crate) fn processes_with_unnotified_status_change(&self) -> Vec<ProcessId> {
        let mut ids: Vec<ProcessId> = self
            .processes
            .iter()
            .filter_map(|(id, proc)| proc.status_ticks.is_unnotified().then_some(*id))
            .collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids
    }
}

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
static STATUS_NOTIFY_WALKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static STATUS_NOTIFY_STAMPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Processes GNU's `p->tick != p->update_tick` (src/process.c:7892) put in a
/// `status_notify` visit set.  See [`STATUS_NOTIFY_WALKS`].
static STATUS_NOTIFY_VISITED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `(walks, stamped, visited)` since the process started.  See the statics
/// above.
#[cfg(test)]
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
