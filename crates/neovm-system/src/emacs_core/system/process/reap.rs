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
//!   private to this module. All process-table reaps and transfers go through
//!   this owner; the child handle cannot be probed independently.
//! * [`ChildOwnership::Reaped`] carries **no pid**, so
//!   `kill` on a reaped pid is not a case that is checked for: it is a
//!   sentence with no subject.
//! * [`ChildOwnership::Deferred`] also carries no pid: asynchronous deletion
//!   transferred the sole unreaped handle into the deleted-child registry.
//! * every transition to `Reaped` happens *inside* the operation that
//!   performed the reap, so the bit cannot be forgotten the way a hand-set
//!   `bool` can.
//!
//! Each process-table owner belongs to its mutator. The deferred registry
//! contains native handles only and serializes transfers and nonblocking
//! sweeps with a mutex, allowing multiple mutators to collect safely. Signal
//! handlers never access either owner; they only wake the existing wait path.
//!
//! The honest limit, stated the way `os_signal.rs` states its own: Rust does
//! not stop a future author from writing a fresh `libc::waitpid` elsewhere.
//! What this buys is that the *existing* reaping sites in the process table
//! collapse to one owner and that the owner cannot be reached without moving
//! through the state machine.

use super::{Value, process_status_exit_value};
use crate::emacs_core::callproc::{SpawnedChild, SpawnedChildIo};
use neovm_eval::emacs_core::eval::data::process_state::{
    ChildHandle, ChildOwnership, ReapableChild,
};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// GNU's deleted_pid_list (process.c:7700-7715), containing native state only.
/// One mutex serializes transfers and WNOHANG collection across mutators and
/// the single native collector. Ordinary waits inspect only the fallback flag,
/// so healthy native registrations add no locks or syscalls to Lisp polls.
static DEFERRED_CHILDREN: Mutex<DeferredRegistry> = Mutex::new(DeferredRegistry::new());
static HAS_SAFE_POINT_FALLBACK: AtomicBool = AtomicBool::new(false);
static NEXT_DEFERRED_ID: AtomicUsize = AtomicUsize::new(1);

/// Never reused, even after its entry or completion Arc has been freed. Native
/// event batches can therefore never complete a later child after PID or
/// allocator-address reuse. Zero belongs exclusively to the native wakeup.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DeferredChildId(NonZeroUsize);

impl DeferredChildId {
    pub fn next() -> Option<Self> {
        // Identity allocation alone needs no publication ordering. The mutex
        // publishes each initialized owner/completion before event collection.
        NEXT_DEFERRED_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .ok()
            .and_then(NonZeroUsize::new)
            .map(Self)
    }

    fn from_native(token: usize) -> Option<Self> {
        NonZeroUsize::new(token).map(Self)
    }
    fn as_native(self) -> usize {
        self.0.get()
    }
}

/// Process-wide native collector state, serialized by the ownership mutex.
/// A failed source cannot accidentally spawn a second collector while the first
/// unwinds: it permanently selects safe-point fallback in production.
#[derive(Debug)]
enum NativeReaper {
    Dormant,
    Active(Arc<super::sys::ChildExitEvent>),
    Failed,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
enum TestReaperLifecycle {
    Serving,
    Finishing,
}

#[derive(Debug)]
struct DeferredRegistry {
    children: Option<HashMap<DeferredChildId, DeferredChild>>,
    fallback: Vec<DeferredChild>,
    native: NativeReaper,
    newly_registered: Vec<DeferredChildId>,
    awaiting_zombie: Option<HashSet<DeferredChildId>>,
    #[cfg(any(test, feature = "test-support"))]
    watcher: Option<std::thread::JoinHandle<()>>,
    #[cfg(any(test, feature = "test-support"))]
    lifecycle: TestReaperLifecycle,
}

impl DeferredRegistry {
    const fn new() -> Self {
        Self {
            children: None,
            fallback: Vec::new(),
            native: NativeReaper::Dormant,
            newly_registered: Vec::new(),
            awaiting_zombie: None,
            #[cfg(any(test, feature = "test-support"))]
            watcher: None,
            #[cfg(any(test, feature = "test-support"))]
            lifecycle: TestReaperLifecycle::Serving,
        }
    }

    fn children(&mut self) -> &mut HashMap<DeferredChildId, DeferredChild> {
        self.children.get_or_insert_with(HashMap::new)
    }

    fn publish(
        &mut self,
        owner: ReapableChild,
        driver: DeferredReapDriver,
    ) -> (Arc<AtomicBool>, Option<DeferredChildId>) {
        let completed = Arc::new(AtomicBool::new(false));
        let child = DeferredChild {
            owner,
            completed: Arc::clone(&completed),
        };
        let id = match driver {
            DeferredReapDriver::NativeExit => DeferredChildId::next(),
            #[cfg(any(test, feature = "test-support"))]
            DeferredReapDriver::WaitSafePointFallback => None,
        };
        match id {
            Some(id) => {
                self.children().insert(id, child);
            }
            None => {
                self.fallback.push(child);
                self.refresh_fallback_flag();
            }
        }
        (completed, id)
    }

    fn refresh_fallback_flag(&self) {
        HAS_SAFE_POINT_FALLBACK.store(!self.fallback.is_empty(), Ordering::Release);
    }

    fn use_fallback(&mut self, id: DeferredChildId) {
        if let Some(child) = self.children().remove(&id) {
            if let NativeReaper::Active(native) = &self.native {
                native.remove(id.as_native(), child.owner.pid);
            }
            if let Some(awaiting) = &mut self.awaiting_zombie {
                awaiting.remove(&id);
            }
            self.fallback.push(child);
            self.refresh_fallback_flag();
        }
    }

    fn collect_one(&mut self, id: DeferredChildId) {
        let Some(child) = self.children().get_mut(&id) else {
            return;
        };
        if !child.owner.try_reap_deleted() {
            return;
        }
        let Some(child) = self.children().remove(&id) else {
            return;
        };
        if let NativeReaper::Active(native) = &self.native {
            native.remove(id.as_native(), child.owner.pid);
        }
        if let Some(awaiting) = &mut self.awaiting_zombie {
            awaiting.remove(&id);
        }
        child.completed.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug)]
enum DeferredReapDriver {
    NativeExit,
    #[cfg(any(test, feature = "test-support"))]
    WaitSafePointFallback,
}

#[derive(Clone, Copy, Debug)]
enum DeferredReapSweep {
    #[cfg(any(test, feature = "test-support"))]
    NativeExit,
    WaitSafePoint,
}

/// The registry retains sole child ownership; observers share only completion,
/// never a handle or numeric authority to wait or signal. Release publishes the
/// completed reap, and scoped observers acquire it before checking resources.
struct DeferredChild {
    owner: ReapableChild,
    completed: Arc<AtomicBool>,
}

impl std::fmt::Debug for DeferredChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeferredChild")
            .field("pid", &self.owner.pid)
            .field("completed", &self.completed)
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, feature = "test-support"))]
static DEFERRED_WATCHER_SCOPE: AtomicBool = AtomicBool::new(false);

/// Process-wide test ownership of the shared worker's explicit shutdown/join.
/// The scope must outlive all process I/O and mutators belonging to its test.
/// Default tests keep the persistent worker just like production; scopes stop
/// it after native completion, leaving no finished unjoined worker.
#[cfg(any(test, feature = "test-support"))]
#[must_use]
#[derive(Debug)]
pub(super) struct DeferredWatcherTestScope {
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(DeferredWatcherTestScope: Send, Sync);

#[cfg(any(test, feature = "test-support"))]
impl DeferredWatcherTestScope {
    pub(super) fn new() -> Self {
        assert!(
            !DEFERRED_WATCHER_SCOPE.swap(true, Ordering::AcqRel),
            "native watcher test scopes cannot overlap"
        );
        Self {
            _thread_bound: std::marker::PhantomData,
        }
    }

    /// Test-only startup count, independent of native thread scheduling.
    pub(super) fn worker_count_for_test(&self) -> usize {
        usize::from(
            DEFERRED_CHILDREN
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .watcher
                .is_some(),
        )
    }

    /// Stop and join the collector after it completes all native entries. This
    /// never drives a child-status sweep on the test's behalf.
    pub(super) fn finish(self) {
        assert!(
            stop_test_reaper().is_ok(),
            "native deleted-child collector panicked"
        );
    }
}

#[cfg(any(test, feature = "test-support"))]
fn stop_test_reaper() -> std::thread::Result<()> {
    let handle = {
        let mut registry = DEFERRED_CHILDREN
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        registry.lifecycle = TestReaperLifecycle::Finishing;
        if let NativeReaper::Active(native) = &registry.native {
            let _ = native.wake();
        }
        registry.watcher.take()
    };
    let result = handle.map_or(Ok(()), std::thread::JoinHandle::join);
    let mut registry = DEFERRED_CHILDREN
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    registry.native = NativeReaper::Dormant;
    registry.lifecycle = TestReaperLifecycle::Serving;
    result
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for DeferredWatcherTestScope {
    fn drop(&mut self) {
        let _ = stop_test_reaper();
        DEFERRED_WATCHER_SCOPE.store(false, Ordering::Release);
    }
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn deferred_child_is_owned_for_test(pid: u32) -> bool {
    let mut registry = DEFERRED_CHILDREN
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    registry
        .children()
        .values()
        .any(|child| child.owner.pid == pid)
        || registry.fallback.iter().any(|child| child.owner.pid == pid)
}

#[cfg(any(test, feature = "test-support"))]
fn publish_deferred_child(child: ReapableChild, driver: DeferredReapDriver) -> Arc<AtomicBool> {
    DEFERRED_CHILDREN
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .publish(child, driver)
        .0
}

fn start_shared_reaper(
    registry: &mut DeferredRegistry,
) -> std::io::Result<Arc<super::sys::ChildExitEvent>> {
    match &registry.native {
        NativeReaper::Active(native) => return Ok(Arc::clone(native)),
        NativeReaper::Failed => {
            return Err(std::io::Error::other(
                "shared deleted-child native source failed",
            ));
        }
        NativeReaper::Dormant => {}
    }
    let native = Arc::new(super::sys::ChildExitEvent::open()?);
    let worker_native = Arc::clone(&native);
    let handle = std::thread::Builder::new()
        .name("neovm-child-reap".into())
        .spawn(move || shared_reaper(worker_native))?;
    registry.native = NativeReaper::Active(Arc::clone(&native));
    #[cfg(any(test, feature = "test-support"))]
    {
        registry.watcher = Some(handle);
    }
    #[cfg(not(any(test, feature = "test-support")))]
    drop(handle);
    Ok(native)
}

fn shared_reaper(native: Arc<super::sys::ChildExitEvent>) {
    loop {
        let events = match native.wait() {
            Ok(events) => events,
            Err(error) => {
                let mut registry = DEFERRED_CHILDREN
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let ids: Vec<_> = registry.children().keys().copied().collect();
                for id in ids {
                    registry.use_fallback(id);
                }
                registry.native = NativeReaper::Failed;
                #[cfg(any(test, feature = "test-support"))]
                if !DEFERRED_WATCHER_SCOPE.load(Ordering::Acquire) {
                    // The worker is about to finish. Default tests detach this
                    // handle rather than retaining a finished unjoined thread;
                    // an active scope retains it for its explicit finish/join.
                    drop(registry.watcher.take());
                }
                tracing::warn!(%error, "shared deleted-child native wait failed; retaining deferred ownership");
                return;
            }
        };
        let mut registry = DEFERRED_CHILDREN
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        collect_native_notifications(&mut registry, events);
        #[cfg(any(test, feature = "test-support"))]
        if matches!(registry.lifecycle, TestReaperLifecycle::Finishing)
            && registry.children().is_empty()
        {
            return;
        }
    }
}

fn collect_native_notifications(
    registry: &mut DeferredRegistry,
    events: Vec<super::sys::ChildExitNotification>,
) {
    let mut ready = std::mem::take(&mut registry.newly_registered)
        .into_iter()
        .collect::<HashSet<_>>();
    let mut retry_zombies = false;
    for event in events {
        match event {
            super::sys::ChildExitNotification::Exited(token) => {
                if let Some(id) = DeferredChildId::from_native(token)
                    && registry.children().contains_key(&id)
                {
                    ready.insert(id);
                    registry
                        .awaiting_zombie
                        .get_or_insert_with(HashSet::new)
                        .insert(id);
                }
            }
            super::sys::ChildExitNotification::ChildSignal => retry_zombies = true,
            super::sys::ChildExitNotification::Wake => {}
        }
    }
    // Darwin NOTE_EXIT precedes SZOMB. Retry only entries whose exit edge has
    // happened, once per SIGCHLD batch; running registrations are never swept.
    if retry_zombies {
        ready.extend(registry.awaiting_zombie.iter().flatten().copied());
    }
    for id in ready {
        registry.collect_one(id);
    }
}

fn defer_child(child: ReapableChild) {
    let mut registry = DEFERRED_CHILDREN
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    // Publish and register while holding one ownership lock. The worker cannot
    // reap/recycle this pid until the source and completion identity both exist.
    let (_, id) = registry.publish(child, DeferredReapDriver::NativeExit);
    let Some(id) = id else {
        return;
    }; // Checked identity exhaustion uses fallback.
    let result = start_shared_reaper(&mut registry).and_then(|native| {
        let Some(child) = registry.children().get(&id) else {
            return Err(std::io::Error::other("missing deferred child registration"));
        };
        native.register(child.owner.pid, id.as_native())
    });
    match result {
        Ok(notification) => {
            registry.newly_registered.push(id);
            match notification {
                super::sys::ChildExitNotification::Exited(_) => {
                    registry
                        .awaiting_zombie
                        .get_or_insert_with(HashSet::new)
                        .insert(id);
                }
                super::sys::ChildExitNotification::ChildSignal
                | super::sys::ChildExitNotification::Wake => {}
            }
        }
        Err(error) => {
            registry.use_fallback(id);
            tracing::warn!(%error, "shared deleted-child native registration failed; retaining deferred ownership");
        }
    }
}

/// Only native-source failures need the ordinary wait's WNOHANG fallback.
/// Native-managed children add no locks or syscalls to zero-timeout Lisp polls.
pub(super) fn collect_deferred_child_status_changes() {
    collect_deferred_children(DeferredReapSweep::WaitSafePoint);
}

fn collect_deferred_children(sweep: DeferredReapSweep) {
    match sweep {
        DeferredReapSweep::WaitSafePoint if !HAS_SAFE_POINT_FALLBACK.load(Ordering::Acquire) => {
            return;
        }
        DeferredReapSweep::WaitSafePoint => {}
        #[cfg(any(test, feature = "test-support"))]
        DeferredReapSweep::NativeExit => {}
    }
    let mut registry = DEFERRED_CHILDREN
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    #[cfg(any(test, feature = "test-support"))]
    if matches!(sweep, DeferredReapSweep::NativeExit) {
        let ids: Vec<_> = registry.children().keys().copied().collect();
        for id in ids {
            registry.collect_one(id);
        }
    }
    registry.fallback.retain_mut(|child| {
        if child.owner.try_reap_deleted() {
            child.completed.store(true, Ordering::Release);
            false
        } else {
            true
        }
    });
    registry.refresh_fallback_flag();
}

/// Test-support route for the option-A re-homed mock/native-deferred tests:
/// publish an owned child on the native-exit driver without starting the
/// shared reaper. The re-homed tests force the completion state themselves.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn publish_native_deferred_child_for_test(child: ReapableChild) -> Arc<AtomicBool> {
    publish_deferred_child(child, DeferredReapDriver::NativeExit)
}

/// Test-support route: the native-exit sweep that the re-homed tests drive
/// explicitly instead of waiting for a real native source.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn collect_native_deferred_children_for_test() {
    collect_deferred_children(DeferredReapSweep::NativeExit);
}

static_assertions::assert_impl_all!(ReapableChild: Send);

/// What one probe of a child's status found.
///
/// The three arms are GNU's three outcomes at `handle_child_signal`
/// (src/process.c:7741-7757): no change; a change that leaves the child alive
/// (`WIFSTOPPED`/`WIFCONTINUED`, which is why GNU passes
/// `WUNTRACED | WCONTINUED`); and a change that clears `p->alive`
/// (`WIFSIGNALED (status) || WIFEXITED (status)`, :7750-7752).
#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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

#[doc(hidden)]
pub trait ChildOwnershipIo {
    /// Take ownership of a freshly spawned `std::process::Child`.
    fn of_pipe_child(child: SpawnedChild) -> Self;
    /// Take ownership of a freshly spawned `portable_pty` child.
    ///
    /// A child `portable_pty` cannot name a pid for is [`Self::NotAChild`],
    /// because a pidless child is exactly what `eassert (child > 0)` rejects
    /// one frame below GNU's own sweep.
    fn of_pty_child(child: Box<dyn portable_pty::Child + Send + Sync>) -> Self;
    /// GNU's `p->alive`: the pid, and only while this port still owns it.
    ///
    /// Every `kill(2)` aimed at one of this port's own children goes through
    /// here, so `record_kill_process`'s `if (p->alive)` is not a test anyone
    /// has to remember to write.
    fn pid_if_unreaped(&self) -> Option<u32>;
    /// The spawn stored a `portable_pty` handle rather than a
    /// `std::process::Child`.
    fn is_pty_handle(&self) -> bool;
    /// The child's stdin while it exists.  Not a reap, and the only reason the
    /// handle is reachable at all.
    fn stdin_mut(&mut self) -> Option<&mut std::fs::File>;
    /// The child's stdin for the poller registrations, which need the
    /// descriptor and nothing else.
    fn stdin(&self) -> Option<&std::fs::File>;
    /// Whether this process has a pipe child at all -- the `else if let
    /// Some(child) = ...` this port's write path used to spell against the
    /// handle itself.
    fn has_pipe_child(&self) -> bool;
    /// GNU's `process-send-eof` for a pipe child: close stdin.  Answers
    /// whether there was a pipe child to close it on.
    fn close_stdin(&mut self) -> bool;
    /// GNU's `child_status_changed (p->pid, &status, WUNTRACED | WCONTINUED)`
    /// (src/process.c:7742), and the state transition its terminal answers
    /// cause.
    ///
    /// [`ChildStatusChange::NoChange`] for a process that is not, or is no
    /// longer, this port's to wait for -- which is GNU's `p->alive &&` at
    /// :7741 rather than an early `continue` inside the loop.
    fn poll_status(&mut self) -> ChildStatusChange;
    /// GNU's `record_kill_process` (src/callproc.c:196-211): signal, then leave
    /// the sole child handle for a later nonblocking deleted-pid sweep.
    ///
    /// The `if (p->alive)` is the `let ... else`, and there is no other arm: a
    /// reaped child is neither signalled nor waited a second time.
    fn terminate_and_defer_reaping(&mut self, kill_signal: i32);
    /// GNU asynchronous delete records an unreaped pid instead of waiting
    /// (callproc.c:202-207). A Darwin child can still be draining its terminal
    /// during exit, so blocking here before closing the master can deadlock.
    fn defer_reaping(&mut self);
    #[cfg(any(test, feature = "test-support"))]
    #[cfg(any(test, feature = "test-support"))]
    fn defer_reaping_for_test(&mut self);
    fn take_unreaped_for_transfer(&mut self) -> Option<ReapableChild>;
    /// The handle's own `kill`, for the callers that fall back to it when the
    /// process group would not take the signal.
    fn kill_handle(&mut self);
}

impl ChildOwnershipIo for ChildOwnership {
    /// Take ownership of a freshly spawned `std::process::Child`.
    fn of_pipe_child(child: SpawnedChild) -> Self {
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
    fn of_pty_child(child: Box<dyn portable_pty::Child + Send + Sync>) -> Self {
        match child.process_id() {
            Some(pid) => Self::Unreaped(ReapableChild {
                pid,
                handle: ChildHandle::Pty(child),
            }),
            None => Self::NotAChild,
        }
    }

    /// GNU's `p->alive`: the pid, and only while this port still owns it.
    ///
    /// Every `kill(2)` aimed at one of this port's own children goes through
    /// here, so `record_kill_process`'s `if (p->alive)` is not a test anyone
    /// has to remember to write.
    fn pid_if_unreaped(&self) -> Option<u32> {
        match self {
            Self::Unreaped(child) => Some(child.pid),
            Self::NotAChild | Self::Reaped | Self::Deferred => None,
        }
    }

    /// The spawn stored a `portable_pty` handle rather than a
    /// `std::process::Child`.
    fn is_pty_handle(&self) -> bool {
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
    fn stdin_mut(&mut self) -> Option<&mut std::fs::File> {
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
    fn stdin(&self) -> Option<&std::fs::File> {
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
    fn has_pipe_child(&self) -> bool {
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
    fn close_stdin(&mut self) -> bool {
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
    fn poll_status(&mut self) -> ChildStatusChange {
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

    /// GNU's `record_kill_process` (src/callproc.c:196-211): signal, then leave
    /// the sole child handle for a later nonblocking deleted-pid sweep.
    ///
    /// The `if (p->alive)` is the `let ... else`, and there is no other arm: a
    /// reaped child is neither signalled nor waited a second time.
    fn terminate_and_defer_reaping(&mut self, kill_signal: i32) {
        let Self::Unreaped(child) = self else {
            return;
        };
        // One probe, through the owner, so an already-exited child is settled
        // rather than signalled.  GNU reaches the same place from the other
        // side: its handler has already cleared `p->alive`.
        if child.try_reap_deleted() {
            *self = Self::Reaped;
            return;
        }
        child.kill_group_then_handle(kill_signal);
        self.defer_reaping();
    }

    /// GNU asynchronous delete records an unreaped pid instead of waiting
    /// (callproc.c:202-207). A Darwin child can still be draining its terminal
    /// during exit, so blocking here before closing the master can deadlock.
    fn defer_reaping(&mut self) {
        if let Some(child) = self.take_unreaped_for_transfer() {
            defer_child(child);
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn defer_reaping_for_test(&mut self) {
        // Synthetic handles have no OS child. Exercise the same ownership
        // transfer without registering native events against their metadata.
        if let Some(child) = self.take_unreaped_for_transfer() {
            let _ = publish_deferred_child(child, DeferredReapDriver::WaitSafePointFallback);
        }
    }

    fn take_unreaped_for_transfer(&mut self) -> Option<ReapableChild> {
        match std::mem::replace(self, Self::Deferred) {
            Self::Unreaped(child) => Some(child),
            unchanged @ (Self::NotAChild | Self::Reaped | Self::Deferred) => {
                *self = unchanged;
                None
            }
        }
    }

    /// The handle's own `kill`, for the callers that fall back to it when the
    /// process group would not take the signal.
    fn kill_handle(&mut self) {
        let Self::Unreaped(child) = self else {
            return;
        };
        child.kill_handle();
    }
}

pub(crate) trait ReapableChildIo {
    /// One non-blocking probe, decoded into the port's status vocabulary.
    ///
    /// The dispatch is the one `poll_child_status_change` already had, kept
    /// exactly: a `portable_pty` child is probed through its own handle, and
    /// everything else through the raw `waitpid` -- which is what carries
    /// `WUNTRACED | WCONTINUED`, so the stop and continue transitions GNU asks
    /// for at :7742 stay visible.
    fn probe(&mut self) -> ChildStatusChange;
    #[cfg(unix)]
    #[cfg(unix)]
    fn probe_pipe(pid: u32, _child: &mut SpawnedChild) -> ChildStatusChange;
    #[cfg(not(unix))]
    #[cfg(not(unix))]
    fn probe_pipe(_pid: u32, child: &mut SpawnedChild) -> ChildStatusChange;
    /// Probe without allocating Lisp values: deleted children have no pending
    /// status or sentinel to publish. Errors other than ECHILD retain ownership
    /// for a later sweep. EINTR resumes immediately so a one-shot native exit
    /// event cannot be consumed without attempting the reap again.
    fn try_reap_deleted(&mut self) -> bool;
    /// GNU's `kill (- p->pid, SIGKILL)` (src/callproc.c:206), with this port's
    /// existing fallback to the handle's own kill when the group does not take
    /// it.
    fn kill_group_then_handle(&mut self, signal_num: i32);
    fn kill_handle(&mut self);
}

impl ReapableChildIo for ReapableChild {
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
    fn probe_pipe(pid: u32, _child: &mut SpawnedChild) -> ChildStatusChange {
        super::process_child_status_change_from_wait(super::sys::poll_child_status(pid))
    }

    #[cfg(not(unix))]
    fn probe_pipe(_pid: u32, child: &mut SpawnedChild) -> ChildStatusChange {
        match child.try_wait() {
            Ok(Some(status)) => ChildStatusChange::Reaped(super::process_status_from_exit(&status)),
            Ok(None) => ChildStatusChange::NoChange,
            Err(_) => ChildStatusChange::Reaped(process_status_exit_value(1)),
        }
    }

    /// Probe without allocating Lisp values: deleted children have no pending
    /// status or sentinel to publish. Errors other than ECHILD retain ownership
    /// for a later sweep. EINTR resumes immediately so a one-shot native exit
    /// event cannot be consumed without attempting the reap again.
    fn try_reap_deleted(&mut self) -> bool {
        loop {
            match &mut self.handle {
                #[cfg(unix)]
                ChildHandle::Pipe(_) => {
                    use super::sys::ChildWait;
                    match super::sys::poll_child_status(self.pid) {
                        ChildWait::Exited(_) | ChildWait::Signaled { .. } | ChildWait::NoChild => {
                            return true;
                        }
                        ChildWait::Running
                        | ChildWait::Stopped(_)
                        | ChildWait::Continued
                        | ChildWait::Undecoded => return false,
                        ChildWait::Error => {
                            if std::io::Error::last_os_error().kind()
                                != std::io::ErrorKind::Interrupted
                            {
                                return false;
                            }
                        }
                    }
                }
                #[cfg(not(unix))]
                ChildHandle::Pipe(child) => match child.try_wait() {
                    Ok(Some(_)) => return true,
                    Ok(None) => return false,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return false,
                },
                ChildHandle::Pty(child) => match child.try_wait() {
                    Ok(Some(_)) => return true,
                    Ok(None) => return false,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        #[cfg(unix)]
                        {
                            return error.raw_os_error() == Some(libc::ECHILD);
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = error;
                            return false;
                        }
                    }
                },
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

#[cfg(test)]
#[path = "tests/deferred_reap.rs"]
mod deferred_reap_tests;
