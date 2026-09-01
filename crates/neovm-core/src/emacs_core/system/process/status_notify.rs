//! GNU `status_notify`'s per-process terminal step, as a type.
//!
//! # The ordering this module exists to make unrepresentable
//!
//! GNU settles a process's presence in `Vprocess_alist` **before** it runs the
//! sentinel.  `status_notify` (src/process.c:7872) does, for each process whose
//! tick changed:
//!
//! ```c
//!       if (p->raw_status_new)                       /* :7914 */
//!         update_status (p);
//!       msg = status_message (p);                    /* :7916 */
//!
//!       symbol = p->status;                          /* :7919 */
//!       if (CONSP (p->status)) symbol = XCAR (p->status);
//!
//!       if (EQ (symbol, Qsignal) || EQ (symbol, Qexit)
//!           || EQ (symbol, Qclosed))                 /* :7923-7924 */
//!         {
//!           if (delete_exited_processes)
//!             remove_process (proc);                 /* :7927 */
//!           else
//!             deactivate_process (proc);             /* :7929 */
//!         }
//!
//!       p->update_tick = p->tick;                    /* :7935 */
//!       exec_sentinel (proc, msg);                   /* :7937 */
//! ```
//!
//! `get-buffer-process` (:8425-8427), `get-process` and `process-list` all walk
//! `Vprocess_alist`, and `remove_process` (:957-966) is the only thing that
//! rewrites it -- `deactivate_process` (:4812) closes descriptors and leaves the
//! alist alone.  So the removal decision is directly observable from inside the
//! sentinel, and it answers differently depending on `delete-exited-processes`
//! (:8916-8920, default 1).  Measured, `emacs -Q --batch`, GNU Emacs 31.0.90:
//!
//! ```text
//! ;; default (delete-exited-processes t)
//! PW169-CHILD-SENTINEL: (:get-buffer-process nil :get-process nil
//!                        :in-process-list nil :process-status exit)
//! ;; (let ((delete-exited-processes nil)) ...)
//! PW169-KEEP-SENTINEL:  (:get-buffer-process t   :get-process t
//!                        :in-process-list t   :process-status exit)
//! ```
//!
//! This port ran the sentinel first and retired afterwards, so an exit sentinel
//! saw its own process in `process-list` under both settings (ledger entry 165,
//! "Found and NOT fixed" 1).
//!
//! # Why GNU can remove first, and what that costs a port
//!
//! `exec_sentinel` is handed the `Lisp_Object proc` the notification loop
//! already holds and passes it straight through (`list3 (sentinel, proc,
//! reason)`, :7845-7846).  Nothing between `remove_process` and the sentinel
//! re-derives the process from a name or a table: in GNU the process IDENTITY
//! is the object, and `Vprocess_alist` is a *directory*, not the storage.  A
//! removed process therefore still answers every accessor -- measured on the
//! value captured inside a GNU sentinel:
//!
//! ```text
//! PW169-REAPED-VALUE: (:eq-to-original t :processp t :name "pw169-val"
//!  :status exit :exit 0 :buffer t :sentinel t :command ("sh" "-c" "printf hi")
//!  :type real :contact t :query-on-exit t :plist nil :tty "/dev/pts/31")
//! ```
//!
//! This port's identity is a [`ProcessId`], resolved through
//! [`ProcessManager`]'s two tables.  Retirement moves the [`Process`] from the
//! live table to the deleted table, and the `get_any`/`get_any_mut` accessors
//! read both -- so the identity survives, exactly as GNU's does.  What does
//! *not* survive is a `get`/`get_mut` lookup: those read the live table only and
//! answer `None`, and every such call in the old code sat behind an
//! `unwrap_or(Value::NIL)` or an `unwrap_or_else(|| "finished\n")`.  Moving the
//! retirement earlier without moving those reads would have replaced a visible
//! ordering bug with a silent one: a sentinel that is `nil` and a message that
//! is always `"finished\n"`.
//!
//! # The type
//!
//! [`ProcessStatusNotification`] is the `exec_sentinel` call of :7937, as a
//! value.  Its fields are private to this module, so no struct literal can be
//! written in `process.rs`, and its only constructors are the two functions
//! below -- **each of which performs the retirement before it returns**.  A
//! caller therefore cannot run a terminal sentinel before the process has been
//! retired, and cannot go looking for the sentinel arguments after the
//! retirement has made them unreachable through `get`.  The ordering is not
//! checked; it is the only order that compiles.

use super::{
    ExitedProcessDisposition, Process, ProcessId, ProcessIoTeardown, ProcessManager, Value,
    gnu_process_status_message_for_process, process_status_is_terminal_for_notify,
};

/// The `exec_sentinel (proc, msg)` GNU makes at src/process.c:7937, captured
/// from the process object while it was still in hand.
///
/// Constructing one of these retires the process; see the module docstring.
/// There is deliberately no other way to obtain the pair, and no accessor that
/// reaches back into the process tables.
#[must_use = "retiring a process is only half of GNU's status_notify; its sentinel must still run"]
pub(super) struct ProcessStatusNotification {
    id: ProcessId,
    /// GNU reads `p->sentinel` inside `exec_sentinel` (:7823) -- after the
    /// removal, which is harmless there because `p` is the object.  This port
    /// reads it before, because `get` would answer `None` after.
    sentinel: Value,
    /// GNU `status_message (p)` (:7916), built from the SETTLED status.
    message: String,
}

impl ProcessStatusNotification {
    /// GNU `status_notify`'s per-process body between the output drain and
    /// `exec_sentinel` (src/process.c:7913-7937), for a process whose terminal
    /// status this port publishes from the wait loop.
    ///
    /// In order: apply the pending raw status (GNU `update_status`, :7914-7915
    /// -- this port's `pending_status`/`status_notify_pending` pair is GNU's
    /// `raw_status`/`raw_status_new`), build the message (:7916), and then, only
    /// when the settled status is terminal (:7919-7924), take the
    /// `delete-exited-processes` decision (:7926-7929).
    ///
    /// Returns `None` for an id that names no live process, which is the
    /// analogue of `FOR_EACH_PROCESS` simply not visiting it.
    pub(super) fn settle_status_and_retire(
        processes: &mut ProcessManager,
        id: ProcessId,
        teardown: ProcessIoTeardown,
        disposition: ExitedProcessDisposition,
    ) -> Option<Self> {
        let (notification, terminal) = {
            let proc: &mut Process = processes.get_mut(id)?;
            if !proc.pending_status.is_nil() {
                proc.status = proc.pending_status;
            }
            (
                Self {
                    id,
                    sentinel: proc.sentinel,
                    message: gnu_process_status_message_for_process(proc),
                },
                process_status_is_terminal_for_notify(&proc.status),
            )
        };
        // GNU `update_status` clears `p->raw_status_new` in the same breath as
        // it converts the raw status (src/process.c:717-721), so the settled
        // status is published exactly once.
        processes.clear_status_notify_pending(id);
        // GNU's SECOND `p->update_tick = p->tick;` (:7935), with its own
        // comment: *"The actions above may have further incremented p->tick.
        // So set p->update_tick again so that an error in the sentinel will
        // not cause this code to be run again."*  It is a separate assignment
        // from :7894 rather than a duplicate: the output drain between them
        // can reach `read_process_output`'s own tick sites (:6075, :6084).
        processes.mark_status_change_notified(id);

        if terminal {
            processes.apply_process_io_teardown(id, teardown);
            match disposition {
                ExitedProcessDisposition::Remove => processes.reap_exited_process(id),
                ExitedProcessDisposition::Deactivate => {}
            }
        }
        Some(notification)
    }

    /// The same `exec_sentinel` call for a notification that retires nothing.
    ///
    /// Only `continue-process` uses it: GNU's `Fcontinue_process` sends SIGCONT
    /// (src/process.c:7287) and the `"continued"` status that follows is not
    /// one of `status_notify`'s terminal symbols (:7923-7924), so no removal
    /// decision is due.  `delete-process` and `kill-buffer` do NOT come here --
    /// they go through `settle_status_and_retire` like everything else, because
    /// GNU's `Fdelete_process` reaches the sentinel through `status_notify`
    /// (:1129 / :1149) and therefore takes the same removal decision.
    ///
    /// Reads through `get_any`, so it answers for a process the caller has
    /// already put in the deleted table.
    pub(super) fn for_retired_process(processes: &ProcessManager, id: ProcessId) -> Option<Self> {
        let proc = processes.get_any(id)?;
        Some(Self {
            id,
            sentinel: proc.sentinel,
            message: gnu_process_status_message_for_process(proc),
        })
    }

    pub(super) fn id(&self) -> ProcessId {
        self.id
    }

    pub(super) fn sentinel(&self) -> Value {
        self.sentinel
    }

    pub(super) fn message(&self) -> &str {
        &self.message
    }
}
