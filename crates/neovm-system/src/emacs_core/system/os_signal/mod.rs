//! GNU's OS signal dispositions, and the one rule that decides their shape:
//! **what a signal handler is allowed to touch.**
//!
//! Before this module, `grep -rn 'sigaction|signal_hook|SIGWINCH|SIGINT'` over
//! `neovm-core`, `neomacs-bin`, `neomacs-display-runtime` and `neovm-worker`
//! returned **zero production hits** (ledger 180 §2, ledger 184).  This port
//! installed no OS signal handler at all, so every signal whose default
//! disposition is `Term` simply killed the editor.  Measured, `-Q --batch`,
//! at a process spinning in pure Lisp:
//!
//! ```text
//!              GNU 31.0.90                     this port, before
//!   SIGUSR2    rc=0, debug-on-quit t           rc=140, killed
//!   SIGUSR1    rc=0, nothing armed             rc=138, killed
//! ```
//!
//! # What GNU installs, and where
//!
//! `init_signals` (src/sysdep.c) is called from `main` after the image is in
//! memory, and its user-signal tail is:
//!
//! ```c
//!   /* SIGUSR1 and SIGUSR2 are used internally by the android_select
//!      function.  */
//!   #if !defined HAVE_ANDROID
//!   #ifdef SIGUSR1
//!     add_user_signal (SIGUSR1, "sigusr1");
//!   #endif
//!   #ifdef SIGUSR2
//!     add_user_signal (SIGUSR2, "sigusr2");
//!   #endif
//!   #endif
//! ```
//!
//! and `add_user_signal` (src/keyboard.c:8464-8483) ends with
//!
//! ```c
//!   emacs_sigaction_init (&action, deliver_user_signal);
//!   sigaction (sig, &action, 0);
//! ```
//!
//! # What a handler may legitimately do -- the whole design input
//!
//! GNU states the rule in the comment block above `handle_child_signal`
//! (src/process.c:7666-7688), and enforces it with a bug report inlined into
//! `child_signal_notify` (:7616-7650):
//!
//! ```text
//!    ** WARNING: this can be called during garbage collection.        /* :7673-7675 */
//!    ** Malloc WARNING: This should never call malloc either directly
//!    or indirectly; if it does, that is a bug.                        /* :7687-7688 */
//!
//!      But this calls `emacs_perror', which in turn invokes a localized
//!      version of strerror, which is not reentrant and must not be
//!      called within a signal handler:
//!        __lll_lock_wait_private () ... malloc () ... strerror_l () ...
//!      So we no longer check errors of emacs_write here.              /* :7630-7649 */
//! ```
//!
//! What survives at the bottom of GNU's handler is one
//! `emacs_write (fd, &dummy, 1)` to a self-pipe.  A fourth constraint is one
//! file over: `deliver_process_signal` (src/sysdep.c:1729-1751) exists because
//! *"POSIX says any thread can receive a signal that is associated with a
//! process"*, and forwards to the main thread with `pthread_kill`.
//!
//! # Why that makes the design, in a port with a render thread
//!
//! GNU's own `handle_user_signal` (src/keyboard.c:8487-8521) is **not**
//! restricted to the async-signal-safe list: it reads `Vdebug_on_event`,
//! calls `SSDATA (SYMBOL_NAME (...))` and writes four Lisp globals.  GNU can
//! do that only because `deliver_process_signal` has already forwarded the
//! signal to the thread that owns those globals.
//!
//! This port cannot forward.  Its Lisp state lives in a `&mut Context` owned
//! by the Lisp thread; there is no `pthread_kill`-shaped way to hand a
//! `&mut Context` to a handler, and a handler that reached the interpreter
//! from the render thread, a worker or the WPE `GMainContext` would be a data
//! race rather than a lock-order problem.  So the split is drawn one step
//! earlier than GNU draws it:
//!
//! * **the handler** does only what GNU's SIGCHLD handler does at its very
//!   bottom -- bump a lock-free counter and write one byte to a self-pipe.
//!   Both are on the POSIX.1-2024 §2.4.3 async-signal-safe list.  It is
//!   therefore correct on *any* thread, which is why this port needs no
//!   analogue of `FORWARD_SIGNAL_TO_MAIN_THREAD`.
//! * **`handle_user_signal`'s body** runs on the Lisp thread at the next safe
//!   point, [`drain_pending_os_signals`], which is where a `&mut Context`
//!   exists.
//!
//! # The type-level part: there is no place to write handler code
//!
//! The bad state this module exists to rule out is "a handler that can touch
//! a non-async-signal-safe thing".  It is made unrepresentable by leaving the
//! handler with nothing to be extended by:
//!
//! * [`HandledSignal`] is a closed, `#[repr(usize)]`, data-only enum whose
//!   `ALL` is declared with length `COUNT` derived from the last discriminant,
//!   so a variant that is not listed is a compile error (ledger 177's
//!   `post_image_init.rs` and ledger 180's `child_status.rs` shape).
//! * [`InstalledDisposition`] is **data, not code**: it carries GNU's
//!   `add_user_signal` NAME and nothing callable.  There is no per-signal
//!   handler hook, no function pointer and no closure anywhere in this
//!   module, so adding a signal adds a citation and a name -- never a line
//!   that runs in signal context.
//! * the one `extern "C"` handler is private, total over `HandledSignal`, and
//!   takes an [`AsyncSignalScope`]: a `!Send`/`!Sync` capability token with no
//!   public constructor whose only two methods are the two POSIX-listed
//!   operations.  **No method on it takes or returns a `Value`**, so a handler
//!   body that reached the interpreter would not type-check.
//!
//! The honest limit of that guarantee is worth stating: Rust lets any function
//! reach a `static`, so this is not a proof that no future author could lock a
//! mutex in signal context.  What it does buy is that the per-signal surface
//! is data and the code is written once -- the unsafe spelling is absent from
//! the type the way `child_status.rs` keeps `waitpid` out of `SweepableChild`.
//!
//! # Where the drain runs, and why that is GNU's own hot path
//!
//! GNU's safe point checks a `volatile bool` next to the quit flag:
//!
//! ```c
//!   /* src/lisp.h:3879, 3896-3900 */
//!   extern bool volatile pending_signals;
//!   INLINE void
//!   maybe_quit (void)
//!   {
//!     if (!NILP (Vquit_flag) || pending_signals)
//!       probably_quit ();
//!   }
//! ```
//!
//! so [`pending`] is one relaxed `'static` load in `Context::maybe_quit`'s
//! fast path -- exactly the shape and exactly the cost GNU pays, and the same
//! shape this port already uses for the Lisp profiler's watchdog flag.

use std::sync::OnceLock;
#[cfg(unix)]
use std::sync::atomic::AtomicI32;
use std::sync::atomic::{AtomicU32, Ordering};

mod platform;

/// One of the OS signals this port installs a disposition for.
///
/// The list is closed and mechanically derivable: `grep -n 'add_user_signal ('
/// src/sysdep.c` gives exactly the two user-signal calls.  Every other signal
/// GNU names in `init_signals` is unclaimed here and recorded as such in
/// ledger 184 -- SIGWINCH, SIGINT and SIGPIPE are all still in the
/// hole ledger 180 §9.6 opened. SIGTERM/SIGHUP instead use GNU's fatal-signal
/// path: capture the numeric status, then call kill-emacs at a safe point.
///
/// **SIGCHLD is deliberately NOT here** (ledger 208).  GNU's
/// `catch_child_signal` (src/process.c:8645-8660) exists to make the child
/// record early and to WAKE the `pselect`; a handler here could do neither.
/// The walk cannot run in a handler (the process table is a `HashMap` owned by
/// the Lisp thread and iterating it allocates, which GNU's own two warnings
/// above `handle_child_signal` forbid), and the wake never existed -- the
/// self-pipe's read end is registered with no poller, and
/// `polling::Poller::wait` catches `ErrorKind::Interrupted` and re-enters,
/// measured at 3.000038747s of a 3s block after a confirmed delivery (ledger
/// 200 §10.1).  GNU exempts exactly this case in its own words: *"WINDOWSNT
/// doesn't need this facility because its 'pselect' emulation ... waits on a
/// subprocess handle, which becomes signaled when the process exits"*
/// (:7548-7552), and this port's `pidfd` is that handle.
///
/// What was left of the trigger was an ARMING condition for the walk -- and
/// GNU does not arm the walk with a signal either.  It arms it with
/// `update_tick != process_tick` (:5524, :5845), a COUNTER, and that arming is
/// only a performance short-circuit: delete it from both of GNU's wait sites
/// and GNU is still correct, because `status_notify`'s body is guarded per
/// process by `p->tick != p->update_tick` (:7892).  Ledger 208 gave this port
/// that per-process pair, ran the walk unconditionally at GNU's own two sites,
/// and the signal then had no job left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum HandledSignal {
    /// `add_user_signal (SIGUSR1, "sigusr1")`.
    Sigusr1 = 0,
    /// `add_user_signal (SIGUSR2, "sigusr2")`.  This is `debug-on-event`'s
    /// default value (src/keyboard.c:14358-14367).
    Sigusr2,
}

/// Where the Lisp thread may act on a delivered signal.
///
/// GNU has two such places and they are different functions with different
/// bodies, which is the distinction ledger 193 collapsed:
///
/// * `process_pending_signals` (src/keyboard.c:8367-8372) --
///   `pending_signals = false; handle_async_input (); do_pending_atimers ();`.
///   `grep -c status_notify` over it is **0**.  It is reached from
///   `maybe_quit` (src/lisp.h:3896-3900 -> `probably_quit`,
///   src/eval.c:1868-1876), so it runs in the middle of arbitrary Lisp, and
///   nothing it does can be observed by a Lisp program.
/// * `wait_reading_process_output`'s `status_notify` calls
///   (src/process.c:5554, :5854), which run sentinels.
///
/// A disposition whose work is invisible to Lisp belongs at the first; one
/// whose work ends in a sentinel belongs at the second.  Putting the second
/// kind at the first is what made a sentinel run after its caller's `let` had
/// unwound (ledger 198).
///
/// The `match` in [`InstalledDisposition::drain_site`] is exhaustive, so a
/// third disposition cannot be added without deciding -- and citing -- which
/// of GNU's two safe points is its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignalDrainSite {
    /// GNU `process_pending_signals`, reached from `maybe_quit`.
    MaybeQuit,
}

impl HandledSignal {
    /// Derived from the last discriminant, so a variant missing from
    /// [`Self::ALL`] is a compile error rather than a silent omission.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const COUNT: usize = Self::Sigusr2 as usize + 1;

    /// Every logical signal implemented by this module.  Platform ownership
    /// is a separate projection; see [`supported_signals`].
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const ALL: [Self; Self::COUNT] = [Self::Sigusr1, Self::Sigusr2];

    /// The OS signal number.
    #[cfg(unix)]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const fn number(self) -> libc::c_int {
        match self {
            Self::Sigusr1 => libc::SIGUSR1,
            Self::Sigusr2 => libc::SIGUSR2,
        }
    }

    /// `file:line` of the install in the GNU tree.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const fn gnu(self) -> &'static str {
        match self {
            Self::Sigusr1 => "src/sysdep.c, init_signals: add_user_signal (SIGUSR1, \"sigusr1\")",
            Self::Sigusr2 => "src/sysdep.c, init_signals: add_user_signal (SIGUSR2, \"sigusr2\")",
        }
    }

    /// What the Lisp thread does with a delivery -- **data only**.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) const fn disposition(self) -> InstalledDisposition {
        match self {
            Self::Sigusr1 => InstalledDisposition::UserSignal {
                lisp_name: "sigusr1",
            },
            Self::Sigusr2 => InstalledDisposition::UserSignal {
                lisp_name: "sigusr2",
            },
        }
    }

    /// The inverse of [`Self::number`], used by the handler.
    ///
    /// The platform capability projection is the source of truth, so Android
    /// can never decode the SIGUSR values it reserves for `android_select`.
    #[cfg(unix)]
    fn from_raw(sig: libc::c_int) -> Option<Self> {
        supported_signals()
            .iter()
            .copied()
            .find(|signal| signal.number() == sig)
    }
}

/// Signals this target permits the editor to own.
///
/// This is deliberately distinct from [`HandledSignal::ALL`]: Windows has no
/// POSIX signal capability, while GNU reserves SIGUSR1/SIGUSR2 on Android for
/// `android_select` and still permits SIGCHLD process notification.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) const fn supported_signals() -> &'static [HandledSignal] {
    platform::SUPPORTED_SIGNALS
}

/// What a delivered signal means to Lisp.
///
/// Deliberately data and not a callback: see the module docs.  A second
/// disposition (SIGWINCH's `change_frame_size`, SIGCHLD's `handle_child_signal`
/// sweep) is a new variant plus one arm in [`drain_pending_os_signals`], and
/// still no new code in signal context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum InstalledDisposition {
    /// GNU's `add_user_signal (sig, NAME)`: the delivery becomes either a
    /// debugger entry or a `USER_SIGNAL_EVENT` carrying `NAME`, decided by
    /// `handle_user_signal` (src/keyboard.c:8487-8521).
    UserSignal { lisp_name: &'static str },
}

impl InstalledDisposition {
    /// Which of GNU's two safe points does this disposition's work.
    ///
    /// The two arms are not a preference; each is read out of GNU:
    ///
    /// * `handle_user_signal`'s non-debugger arm ends `p->npending++;
    ///   pending_signals = true;` (src/keyboard.c:8511-8512), and
    ///   `pending_signals` is exactly what `maybe_quit` tests.
    /// * `handle_child_signal` never touches `pending_signals` -- `grep -n
    ///   'pending_signals = ' src/*.c` returns eleven lines and not one is in
    ///   `process.c`.  Its wake is `child_signal_notify` (:7766-7767), a byte
    ///   on a self-pipe that only the `select` inside
    ///   `wait_reading_process_output` is watching, and its notification is
    ///   that function's `status_notify` (:5554, :5854).
    ///
    /// So the routing is GNU's own wiring, not a judgement call: the two
    /// signals reach the Lisp thread through two different flags in GNU, and
    /// ledger 193's defect was giving them one.
    pub(crate) const fn drain_site(self) -> SignalDrainSite {
        match self {
            Self::UserSignal { .. } => SignalDrainSite::MaybeQuit,
        }
    }
}

/// The disposition that was in place before this port installed its own.
///
/// GNU works around exactly one library that claims a signal Emacs also wants
/// (`lib_child_handler`, src/process.c:7654-7660, for Glib's SIGCHLD).  This
/// records the same fact for the signals installed here, so "nothing else
/// wanted them" is a measurement rather than an assumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum PreviousDisposition {
    /// `SIG_DFL` -- for SIGUSR1/2 that means `Term`, which is the bug.
    Default,
    /// `SIG_IGN`.
    Ignored,
    /// Some other handler was already installed.
    Handler,
    /// `sigaction` could not report it.
    Unknown,
}

/// What [`install`] did, so the install is inspectable rather than assumed.
#[derive(Debug)]
#[doc(hidden)]
pub struct InstallReport {
    previous: [PreviousDisposition; HandledSignal::COUNT],
    installed: [bool; HandledSignal::COUNT],
    /// The owned self-pipe, or `None` on a target without POSIX signals.
    /// On Unix setup failure aborts startup rather than losing idle wakeups.
    ///
    /// Registered by ProcessWaitBackend, including for an idle daemon. EINTR
    /// alone is insufficient: polling retries interrupted waits internally.
    /// The handler's nonblocking write returns that wait even when the signal
    /// lands on another thread. Pending atomics, not bytes, own delivery counts.
    wake_pipe: Option<platform::WakePipe>,
}

impl InstallReport {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn previous(&self, signal: HandledSignal) -> PreviousDisposition {
        self.previous[signal as usize]
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn installed_count(&self) -> usize {
        self.installed.iter().filter(|done| **done).count()
    }

    /// The process-lifetime self-pipe read end for evaluator wait pollers.
    /// `None` on a target without POSIX signals.
    pub fn self_pipe_read_fd(&self) -> Option<libc::c_int> {
        self.wake_pipe
            .as_ref()
            .and_then(platform::WakePipe::read_fd)
    }

    #[cfg(all(any(test, feature = "test-support"), unix))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn self_pipe_fds(&self) -> Option<[libc::c_int; 2]> {
        self.wake_pipe.as_ref().map(|pipe| {
            [
                pipe.read_fd().expect("Unix wake pipe has a read end"),
                pipe.write_fd(),
            ]
        })
    }
}

// ---------------------------------------------------------------------------
// The async-signal-safe half.  Everything below this line runs in signal
// context; everything it may touch is declared here and nowhere else.
// ---------------------------------------------------------------------------

/// Per-signal delivery counter, GNU's `p->npending` (src/keyboard.c:8456,
/// bumped at :8511).
///
/// `AtomicU32` rather than `sig_atomic_t` because a lock-free atomic RMW is
/// what makes the count correct when the signal lands on the render thread.
/// `AtomicU32::fetch_add` compiles to `lock xadd` on this target and takes no
/// lock; `the_pending_counters_are_lock_free` asserts the stable spelling of
/// that fact, `cfg!(target_has_atomic = "32")`, because an atomic that fell
/// back to `core`'s lock would put a lock in signal context.
static PENDING: [AtomicU32; HandledSignal::COUNT] =
    [const { AtomicU32::new(0) }; HandledSignal::COUNT];

/// The write end of GNU's self-pipe (`child_signal_write_fd`,
/// src/process.c:7595).  `-1` until [`install`] creates it.
#[cfg(unix)]
static SELF_PIPE_WRITE_FD: AtomicI32 = AtomicI32::new(-1);

/// GNU emacs.c:fatal_error_signal calls Fkill_emacs with the signal number for
/// SIGTERM/SIGHUP. Keep the first pending request; only the evaluator can
/// consume it. No Lisp state is touched by the handler.
#[cfg(unix)]
static TERMINATION_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// A capability token that exists only for the duration of a signal handler.
///
/// Its two methods are the only two operations this port performs in signal
/// context, and both are on the POSIX.1-2024 §2.4.3 async-signal-safe list.
/// It is `!Send` and `!Sync` (the `PhantomData<*const ()>`), has no public
/// constructor, and **no method takes or returns a Lisp `Value`** -- which is
/// the type-level statement that a handler cannot reach the interpreter.
#[cfg(unix)]
struct AsyncSignalScope(std::marker::PhantomData<*const ()>);

#[cfg(unix)]
impl AsyncSignalScope {
    /// GNU's `p->npending++` (src/keyboard.c:8511).
    ///
    /// A lock-free atomic RMW: no allocation, no lock, no reentrancy hazard.
    ///
    /// **Deliberately separate from [`Self::set_pending_signals`]**, because
    /// GNU keeps them separate: the user-signal handler does both
    /// (:8511-8512) and `handle_child_signal` does NEITHER -- it stamps the
    /// process struct instead, which is the part a Rust port cannot do in
    /// signal context, and wakes with [`Self::wake`].  A SIGCHLD delivery
    /// therefore bumps this counter -- the port's stand-in for the record the
    /// handler cannot make -- and leaves `pending_signals` alone, so it does
    /// not reach `maybe_quit` at all.  That wire is what took ledger 193's
    /// drain to the wrong safe point.
    fn record(&self, signal: HandledSignal) {
        PENDING[signal as usize].fetch_add(1, Ordering::Relaxed);
    }

    fn record_termination(&self, sig: libc::c_int) {
        let _ = TERMINATION_SIGNAL.compare_exchange(0, sig, Ordering::Release, Ordering::Relaxed);
    }

    /// GNU's `pending_signals = true` (src/keyboard.c:8512, :8431), the flag
    /// `maybe_quit` tests.
    ///
    /// The one write this handler makes outside the section above: GNU's
    /// `pending_signals` is the evaluator's asynchronous-attention word
    /// (`eval::ASYNC_ATTENTION`), shared with the profiler tick and the
    /// cross-thread quit request so that a safe point tests all three with one
    /// load.  `raise` is a single lock-free `fetch_or` on an `AtomicU32`
    /// (`the_pending_counters_are_lock_free` pins the width), so it keeps this
    /// method on the async-signal-safe list.
    fn set_pending_signals(&self) {
        neovm_eval::emacs_core::eval::ASYNC_ATTENTION
            .raise(neovm_eval::emacs_core::eval::AsyncSource::OsSignal);
    }

    /// GNU's `child_signal_notify`, whose entire surviving body is
    /// `emacs_write (fd, &dummy, 1)` (src/process.c:7648) -- and whose comment
    /// is the evidence for why nothing more may go here.
    ///
    /// The result is deliberately discarded, for GNU's own stated reason: the
    /// error path it used to have reached `strerror_l` and therefore `malloc`.
    fn wake(&self) {
        let fd = SELF_PIPE_WRITE_FD.load(Ordering::Relaxed);
        if fd < 0 {
            return;
        }
        let dummy: u8 = 0;
        // SAFETY: `write` is async-signal-safe (POSIX.1-2024 §2.4.3); `fd` is
        // the self-pipe's O_NONBLOCK write end, owned for the process's
        // lifetime, and the buffer is one byte of stack.  A short write or
        // EAGAIN is the coalescing this wake wants, so the result is dropped
        // exactly as GNU drops it.
        unsafe {
            libc::write(fd, std::ptr::addr_of!(dummy).cast(), 1);
        }
    }
}

/// The signals GNU's `maybe_fatal_sig` captures as process-fatal
/// (`src/sysdep.c:2044-2046`): SIGHUP, SIGINT, SIGTERM.
///
/// One list, read by both the installer and the handler, so a signal cannot
/// be installed without also being latched — which is exactly how SIGINT was
/// once installed and then silently dropped on delivery.
#[cfg(unix)]
pub(crate) const FATAL_SIGNALS: [libc::c_int; 3] = [libc::SIGHUP, libc::SIGINT, libc::SIGTERM];

#[cfg(unix)]
pub(crate) fn is_fatal_signal(sig: libc::c_int) -> bool {
    FATAL_SIGNALS.contains(&sig)
}

/// GNU's `deliver_user_signal` (src/keyboard.c:8524-8531) with the forwarding
/// removed, because this handler is correct on any thread.
///
/// There is exactly one of these in the crate and it is total over
/// [`supported_signals`], so a new platform-owned signal adds no code that
/// runs here.
#[cfg(unix)]
extern "C" fn deliver_user_signal(sig: libc::c_int) {
    // GNU preserves errno around the handler ("Races can occur even in
    // single-threaded hosts", src/sysdep.c:1734-1735).
    // The platform seam owns the target-specific TLS accessor; reading and
    // restoring it is what GNU does at src/sysdep.c:1733 and :1750.
    let saved_errno = platform::save_errno();

    if is_fatal_signal(sig) {
        let scope = AsyncSignalScope(std::marker::PhantomData);
        scope.record_termination(sig);
        scope.set_pending_signals();
        scope.wake();
    } else if let Some(signal) = HandledSignal::from_raw(sig) {
        let scope = AsyncSignalScope(std::marker::PhantomData);
        match signal.disposition() {
            // Every arm may use only `scope`, and `scope` has only these three
            // operations.  This `match` is the whole handler.
            //
            // The two arms are NOT the same, and that is the finding rather
            // than a detail.  GNU's user-signal handler ends with
            // `p->npending++; pending_signals = true;` (src/keyboard.c:
            // 8511-8512) and writes to no pipe; GNU's SIGCHLD handler ends
            // with `if (changed) child_signal_notify ();` (:7766-7767), whose
            // body is one `emacs_write`, and never assigns `pending_signals`
            // at all.  Two signals, two flags, two safe points -- see
            // [`InstalledDisposition::drain_site`].
            //
            // `record` appears in both because everything GNU does BEFORE
            // those lines needs Lisp state and is therefore deferred here: for
            // a user signal that is `handle_user_signal`'s decision, and for
            // SIGCHLD it is `handle_child_signal`'s walk.  The counter is the
            // deferral, and the two arms differ in what they then wake.
            InstalledDisposition::UserSignal { .. } => {
                scope.record(signal);
                scope.set_pending_signals();
                scope.wake();
            }
        }
    }

    platform::restore_errno(saved_errno);
}

// ---------------------------------------------------------------------------
// Install (runs once, on the Lisp thread, from post_image_init)
// ---------------------------------------------------------------------------

static INSTALL: OnceLock<InstallReport> = OnceLock::new();

/// GNU's `init_signals` user-signal tail, run once per process.
///
/// Called from `post_image_init` because GNU calls `init_signals` from `main`
/// after the image is in memory (`PostImageInit::Signals`, src/emacs.c:1951);
/// idempotent, because `add_user_signal` opens by returning early for a signal
/// it has already added (src/keyboard.c:8470-8473).
#[doc(hidden)]
pub fn install() -> &'static InstallReport {
    INSTALL.get_or_init(install_once)
}

#[cfg(unix)]
fn install_once() -> InstallReport {
    let wake_pipe =
        Some(platform::create_wake_pipe().expect("cannot create evaluator signal wake pipe"));
    if let Some(pipe) = &wake_pipe {
        SELF_PIPE_WRITE_FD.store(pipe.write_fd(), Ordering::Release);
    }

    let mut previous = [PreviousDisposition::Unknown; HandledSignal::COUNT];
    let mut installed = [false; HandledSignal::COUNT];

    for &signal in supported_signals() {
        // GNU's `emacs_sigaction_init` (src/sysdep.c:1678-1710) blocks the
        // nonfatal signals Emacs catches while this one is being handled, "so
        // race conditions are less likely".  The set is reproduced here for
        // the signals this port installs; the rest of GNU's set names signals
        // nothing here catches, so adding them would block a disposition that
        // does not exist.
        //
        // SAFETY: `sigaction` is called with a fully initialised `struct
        // sigaction` and a valid signal number, before any thread that could
        // race with it is started.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        // SAFETY: `sigemptyset`/`sigaddset` write only through `sa_mask`.
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            for &other in supported_signals() {
                libc::sigaddset(&mut action.sa_mask, other.number());
            }
        }
        let handler: extern "C" fn(libc::c_int) = deliver_user_signal;
        action.sa_sigaction = handler as usize;
        // SA_RESTART unconditionally, where GNU takes it only when
        // `noninteractive` (`emacs_sigaction_flags`, src/sysdep.c:1660-1673).
        // GNU's interactive reason for omitting it is "we need to poll for
        // pending input so we need long-running syscalls to be interrupted";
        // this port's wake is the self-pipe instead, and the wait it must
        // interrupt is `epoll_wait`, which signal(7) lists among the calls
        // that are "never restarted after being interrupted by a signal
        // handler, regardless of the use of SA_RESTART".  So SA_RESTART costs
        // nothing here and spares every other blocking call in the render
        // thread and the workers a spurious EINTR.
        action.sa_flags = libc::SA_RESTART;

        // Seeded with a sentinel `sigaction` cannot produce, so
        // "the previous disposition was SIG_DFL" is a reading and not the
        // zero this struct would have had anyway.
        let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
        old.sa_sigaction = usize::MAX;
        // SAFETY: both pointers are to fully initialised locals.
        let rc = unsafe { libc::sigaction(signal.number(), &action, &mut old) };
        let index = signal as usize;
        if rc == 0 {
            installed[index] = true;
            previous[index] = classify_previous(&old);
        }
    }

    let report = InstallReport {
        previous,
        installed,
        wake_pipe,
    };

    // The install is the kind of fact that is invisible until it is wrong, so
    // it says what it did once, with GNU's line for each disposition and the
    // disposition it replaced.  `PreviousDisposition::Handler` here would mean
    // something else in this process wanted the signal -- GNU has exactly one
    // such case and works around it by hand (`lib_child_handler`,
    // src/process.c:7654-7660).
    for &signal in supported_signals() {
        tracing::debug!(
            signal = ?signal,
            number = signal.number(),
            gnu = signal.gnu(),
            previous = ?report.previous(signal),
            self_pipe_read_fd = ?report.self_pipe_read_fd(),
            "init_signals: installed an OS signal disposition"
        );
    }

    report
}

#[cfg(not(unix))]
fn install_once() -> InstallReport {
    InstallReport {
        previous: [PreviousDisposition::Unknown; HandledSignal::COUNT],
        installed: [false; HandledSignal::COUNT],
        wake_pipe: platform::create_wake_pipe(),
    }
}

/// Install termination capture only once the host knows batch/interactive mode.
/// Process-manager construction may create the wake pipe, but must not choose
/// fatal-signal policy. GNU maybe_fatal_sig preserves SIG_IGN in batch mode.
///
/// The set is GNU's: `maybe_fatal_sig` captures SIGHUP, SIGINT and SIGTERM
/// (`src/sysdep.c:2044-2046`).
#[doc(hidden)]
pub fn install_termination(noninteractive: bool) {
    #[cfg(unix)]
    {
        install();
        for sig in FATAL_SIGNALS {
            // SAFETY: valid signals and initialized local sigaction storage.
            let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
            if unsafe { libc::sigaction(sig, std::ptr::null(), &mut old) } != 0 {
                panic!(
                    "cannot inspect shutdown signal {sig}: {}",
                    std::io::Error::last_os_error()
                );
            }
            if noninteractive && old.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = deliver_user_signal as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            // SAFETY: initialized action, valid signal set and signal numbers.
            unsafe {
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaddset(&mut action.sa_mask, libc::SIGTERM);
                libc::sigaddset(&mut action.sa_mask, libc::SIGHUP);
                if libc::sigaction(sig, &action, std::ptr::null_mut()) != 0 {
                    panic!(
                        "cannot install orderly shutdown signal {sig}: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
        // P4: GNU's fatal faults are installed after Rust std's runtime has
        // taken SIGSEGV/SIGBUS for its stack-overflow detector; the install
        // saves std's action so a genuine guard-page hit still chains to it.
        install_fault_dispositions();
    }
    #[cfg(not(unix))]
    let _ = noninteractive;
}

// ---------------------------------------------------------------------------
// GNU fatal faults: SIGSEGV/SIGBUS ownership after Rust std (P4)
// ---------------------------------------------------------------------------
//
// Rust std's rt init installs a SIGSEGV/SIGBUS handler when the inherited
// disposition is SIG_DFL (library/std/src/sys/pal/unix/stack_overflow.rs).
// That handler only acts when the fault address lies in the current thread's
// recorded guard-page range; for anything else it sets SIG_DFL and RETURNS,
// which re-executes the faulting instruction (death by SIG_DFL) for a real
// fault and lets a `kill -SEGV`-style synthetic delivery continue as if
// nothing happened -- the observed swallowed-signal divergence: the parity
// probes' batch editors stayed alive after either signal. GNU instead runs
// terminate_due_to_signal for both
// (src/sysdep.c:1917-1944, handle_sigsegv -> deliver_fatal_thread_signal).
//
// This section re-installs our own handler after std's, saving the previous
// action for the genuine-stack-overflow chain, and classifies the delivery by
// siginfo's si_code: kernel fault codes are positive (SEGV_MAPERR,
// SEGV_ACCERR, BUS_*), every synthetic sender uses a non-positive code
// (SI_USER 0, SI_QUEUE -1, SI_TIMER -2, SI_MESGQ -3, SI_ASYNCIO -4,
// SI_SIGIO -5, SI_TKILL -6). `si_addr == 0` is NOT the discriminator: a real
// null-pointer fault has si_addr 0 too, which is why Rust std's zero check
// treats it as synthetic.

/// GNU's fatal backtrace limit (`terminate_due_to_signal (sig, 40)`,
/// src/emacs.c:440/464); `emacs_backtrace` caps any limit at 500
/// (src/sysdep.c:1757).
#[cfg(unix)]
const FAULT_BACKTRACE_LIMIT: usize = 40;

#[cfg(unix)]
static FAULT_BACKTRACE: [std::sync::atomic::AtomicPtr<libc::c_void>; FAULT_BACKTRACE_LIMIT] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; FAULT_BACKTRACE_LIMIT];

/// How many pointers [`FAULT_BACKTRACE`] holds, written by the handler.
#[cfg(unix)]
static FAULT_NPOINTERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The SIGSEGV action that was installed before ours (Rust std's), so a
/// genuine guard-page fault can chain to the stack-overflow detector.
#[cfg(unix)]
static PREVIOUS_SEGV_ACTION: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(unix)]
static PREVIOUS_SEGV_FLAGS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
#[cfg(unix)]
static PREVIOUS_BUS_ACTION: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(unix)]
static PREVIOUS_BUS_FLAGS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// The install runs once; a second `install_termination` must not record our
/// own action as the previous one.
#[cfg(unix)]
static FAULT_INSTALL: OnceLock<()> = OnceLock::new();

/// See the section comment: kernel fault codes are positive, synthetic
/// delivery codes are non-positive (POSIX `SI_*` values).
#[cfg(unix)]
fn fault_is_synchronous(si_code: libc::c_int) -> bool {
    si_code > 0
}

/// Read `si_code` from the `siginfo_t` a SA_SIGINFO handler received.
///
/// Linux and Darwin both put `si_signo`, `si_errno`, `si_code` as the first
/// three `int`s of `siginfo_t`; `libc`'s own `siginfo_t::si_addr()` uses the
/// same prefix cast (linux/gnu/mod.rs).
#[cfg(unix)]
unsafe fn siginfo_code(info: *const libc::siginfo_t) -> libc::c_int {
    #[repr(C)]
    struct SiginfoPrefix {
        _si_signo: libc::c_int,
        _si_errno: libc::c_int,
        si_code: libc::c_int,
    }
    // SAFETY: `info` is the siginfo_t the kernel passed to this SA_SIGINFO
    // handler; the prefix layout is the ABI's on every supported target.
    unsafe { (*info.cast::<SiginfoPrefix>()).si_code }
}

/// GNU `safe_strsignal`'s descriptions for the signals this port can raise
/// from a fault (src/sysdep.c:2894-2903 routes to `sigdescr_np`). A static
/// table keeps the handler off `strsignal`, which is not async-signal-safe.
#[cfg(unix)]
const fn fatal_signal_description(sig: libc::c_int) -> Option<&'static str> {
    match sig {
        libc::SIGHUP => Some("Hangup"),
        libc::SIGINT => Some("Interrupt"),
        libc::SIGQUIT => Some("Quit"),
        libc::SIGILL => Some("Illegal instruction"),
        libc::SIGTRAP => Some("Trace/breakpoint trap"),
        libc::SIGABRT => Some("Aborted"),
        libc::SIGBUS => Some("Bus error"),
        libc::SIGFPE => Some("Floating point exception"),
        libc::SIGSEGV => Some("Segmentation fault"),
        libc::SIGTERM => Some("Terminated"),
        libc::SIGSYS => Some("Bad system call"),
        libc::SIGVTALRM => Some("Virtual timer expired"),
        libc::SIGXCPU => Some("CPU time limit exceeded"),
        libc::SIGXFSZ => Some("File size limit exceeded"),
        _ => None,
    }
}

/// Write all of `bytes` to `fd`, async-signal-safe (no allocation, no stdio).
#[cfg(unix)]
fn write_all_raw(fd: libc::c_int, bytes: &[u8]) {
    let mut written = 0;
    while written < bytes.len() {
        // SAFETY: `bytes` is live for the call; a short write is retried and
        // an error ends the attempt exactly as GNU's emacs_write diagnostics do.
        let n = unsafe { libc::write(fd, bytes[written..].as_ptr().cast(), bytes.len() - written) };
        if n <= 0 {
            return;
        }
        written += n as usize;
    }
}

/// GNU `shut_down_emacs`'s fatal diagnostic (src/emacs.c:3112-3147): only
/// when this process is the foreground process group of its controlling
/// terminal, and never for SIGTERM (`sig && sig != SIGTERM`).
#[cfg(unix)]
unsafe fn write_fatal_error_line(sig: libc::c_int) {
    if sig == libc::SIGTERM {
        return;
    }
    // SAFETY: tcgetpgrp/getpgrp are async-signal-safe syscalls; a failed
    // tcgetpgrp returns -1 and skips the line exactly as GNU's test does.
    let foreground = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) } == unsafe { libc::getpgrp() };
    if !foreground {
        return;
    }
    let Some(description) = fatal_signal_description(sig) else {
        return;
    };
    let prefix = b"Fatal error ";
    let separator = b": ";
    let mut buf = [0u8; 128];
    let mut at = 0;
    buf[..prefix.len()].copy_from_slice(prefix);
    at += prefix.len();
    let mut value = u32::from(sig.unsigned_abs());
    let mut digits = [0u8; 10];
    let mut ndigits = 0;
    loop {
        digits[ndigits] = b'0' + (value % 10) as u8;
        value /= 10;
        ndigits += 1;
        if value == 0 {
            break;
        }
    }
    for index in (0..ndigits).rev() {
        buf[at] = digits[index];
        at += 1;
    }
    buf[at..at + separator.len()].copy_from_slice(separator);
    at += separator.len();
    let description = description.as_bytes();
    if at + description.len() + 1 > buf.len() {
        return;
    }
    buf[at..at + description.len()].copy_from_slice(description);
    at += description.len();
    buf[at] = b'\n';
    at += 1;
    write_all_raw(libc::STDERR_FILENO, &buf[..at]);
}

/// Capture `backtrace(3)` into the static buffer GNU keeps for fatal
/// signals (src/sysdep.c:1754-1764). The tables are warmed at install time
/// because the first `backtrace` call may allocate (src/emacs.c:1627).
#[cfg(unix)]
unsafe fn capture_fault_backtrace() {
    let mut buffer = [std::ptr::null_mut::<libc::c_void>(); FAULT_BACKTRACE_LIMIT];
    // SAFETY: `buffer` is a live stack array of the length passed.
    let count =
        unsafe { libc::backtrace(buffer.as_mut_ptr(), FAULT_BACKTRACE_LIMIT as libc::c_int) };
    let count = count.max(0) as usize;
    for (slot, pointer) in FAULT_BACKTRACE.iter().zip(buffer.iter()).take(count) {
        slot.store(*pointer, Ordering::Relaxed);
    }
    FAULT_NPOINTERS.store(count, Ordering::Release);
}

/// GNU `emacs_backtrace`'s output half (src/sysdep.c:2336-2369): the marker,
/// then `backtrace_symbols_fd` frames, both async-signal-safe (`glibc`
/// documents that the `_fd` form does not allocate).
#[cfg(unix)]
unsafe fn write_fault_backtrace() {
    let count = FAULT_NPOINTERS
        .load(Ordering::Acquire)
        .min(FAULT_BACKTRACE_LIMIT);
    if count == 0 {
        return;
    }
    let mut pointers = [std::ptr::null_mut::<libc::c_void>(); FAULT_BACKTRACE_LIMIT];
    for (destination, slot) in pointers.iter_mut().zip(FAULT_BACKTRACE.iter()).take(count) {
        *destination = slot.load(Ordering::Relaxed);
    }
    write_all_raw(libc::STDERR_FILENO, b"Backtrace:\n");
    // SAFETY: `pointers` holds `count` valid frame addresses and `count`
    // does not exceed its length.
    unsafe {
        libc::backtrace_symbols_fd(pointers.as_ptr(), count as libc::c_int, libc::STDERR_FILENO);
    }
}

/// The SA_SIGINFO callback a `sigaction` names.
#[cfg(unix)]
type FaultHandler = unsafe extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void);

/// The previous handler for `sig`, when it takes a SA_SIGINFO signature.
#[cfg(unix)]
fn previous_fault_handler(sig: libc::c_int) -> Option<FaultHandler> {
    let (action, flags) = match sig {
        libc::SIGSEGV => (
            PREVIOUS_SEGV_ACTION.load(Ordering::Acquire),
            PREVIOUS_SEGV_FLAGS.load(Ordering::Acquire),
        ),
        libc::SIGBUS => (
            PREVIOUS_BUS_ACTION.load(Ordering::Acquire),
            PREVIOUS_BUS_FLAGS.load(Ordering::Acquire),
        ),
        _ => return None,
    };
    if action == 0 || action == libc::SIG_DFL || action == libc::SIG_IGN {
        return None;
    }
    if flags & libc::SA_SIGINFO == 0 {
        return None;
    }
    // SAFETY: this is the one peer-signed transmute exception for the fatal
    // P4 work (TRANSMUTE_CEILING 6 -> 7, pid f70ad89b). `sa_sigaction` is an
    // integer by ABI and there is no safe route from an integer to a function
    // pointer. The value was read back from a sigaction this process
    // installed; SIG_DFL/SIG_IGN are filtered above and SA_SIGINFO fixes the
    // three-argument signature, so the bits name a valid callback.
    Some(unsafe { std::mem::transmute::<usize, FaultHandler>(action) })
}

/// Die by `sig` the way GNU's `terminate_due_to_signal` ends: unblock it,
/// restore the default action, raise it (src/emacs.c:471-483).
#[cfg(unix)]
unsafe fn raise_fault_now(sig: libc::c_int) -> ! {
    // SAFETY: signal-set manipulation on a local set, then this thread's mask;
    // signal()/raise() are async-signal-safe.
    unsafe {
        let mut unblocked: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut unblocked);
        libc::sigaddset(&mut unblocked, sig);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &unblocked, std::ptr::null_mut());
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
        // GNU's `exit (1)` after emacs_raise is unreachable (emacs.c:482-483);
        // a handler must not return to a faulting instruction, so end here if
        // raise did not.
        libc::abort()
    }
}

/// The handler Rust std's stack-overflow detector is chained from.
///
/// P4 scope: a synthetic delivery (kill-sent) and a real fault the previous
/// handler did not claim both report with the async-signal-safe subset and
/// die by the signal here. P3 replaces this with the FatalPhase latch plus
/// the safe-point `shut_down_emacs` (auto-save), which needs threadsig's A1
/// entry first.
///
/// # Safety
///
/// Installed with SA_SIGINFO; only the kernel calls it, with a valid
/// `siginfo_t` and ucontext for `sig`. It must never be called from safe
/// Rust: its pointers are raw and its body touches process-global signal
/// state.
#[cfg(unix)]
unsafe extern "C" fn deliver_fatal_fault(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    // SAFETY: `info` is the kernel-provided siginfo_t for this delivery.
    let synchronous = unsafe { fault_is_synchronous(siginfo_code(info)) };
    if synchronous {
        // Give Rust std's stack-overflow detector first refusal. It aborts
        // when the address is inside its recorded guard range and otherwise
        // resets SIG_DFL and returns; returning is safe only from the std
        // handler's own frame, so our continuation must not return to the
        // faulting instruction.
        if let Some(previous) = previous_fault_handler(sig) {
            // SAFETY: the saved action carries SA_SIGINFO, so this is the
            // three-argument signature the kernel would have used, and the
            // kernel's own siginfo/context are still live.
            unsafe { previous(sig, info, context) };
        }
    }
    // SAFETY: async-signal-safe subset only: backtrace (warmed), the tty
    // diagnostic, the frame dump, then SIG_DFL + raise.
    unsafe {
        capture_fault_backtrace();
        write_fatal_error_line(sig);
        write_fault_backtrace();
        raise_fault_now(sig);
    }
}

/// Install our SIGSEGV/SIGBUS actions after Rust std's, saving the previous
/// action per signal. Idempotent; called by [`install_termination`], which
/// runs from `main` after the Rust runtime has installed its handler.
#[cfg(unix)]
fn install_fault_dispositions() {
    FAULT_INSTALL.get_or_init(|| {
        // GNU initializes the backtrace tables before any handler can run
        // (`emacs_backtrace (-1)`, src/emacs.c:1627) because the first
        // `backtrace` call may allocate.
        let mut warm = [std::ptr::null_mut::<libc::c_void>(); 1];
        // SAFETY: a one-element stack buffer.
        unsafe { libc::backtrace(warm.as_mut_ptr(), 1) };

        for (sig, previous_action, previous_flags) in [
            (libc::SIGSEGV, &PREVIOUS_SEGV_ACTION, &PREVIOUS_SEGV_FLAGS),
            (libc::SIGBUS, &PREVIOUS_BUS_ACTION, &PREVIOUS_BUS_FLAGS),
        ] {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = deliver_fatal_fault as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
            // SAFETY: both pointers are to initialised locals and `sig` is a
            // valid signal number.
            let installed = unsafe {
                libc::sigfillset(&mut action.sa_mask);
                libc::sigaction(sig, &action, &mut old)
            };
            if installed == 0 {
                previous_action.store(old.sa_sigaction, Ordering::Release);
                previous_flags.store(old.sa_flags, Ordering::Release);
            }
        }
    });
}

#[cfg(unix)]
fn classify_previous(old: &libc::sigaction) -> PreviousDisposition {
    match old.sa_sigaction {
        // The seed survived: `sigaction` reported success without writing the
        // old action, which no implementation should do -- but if one does,
        // the answer must not be the sentinel-shaped `SIG_DFL`.
        handler if handler == usize::MAX => PreviousDisposition::Unknown,
        handler if handler == libc::SIG_DFL => PreviousDisposition::Default,
        handler if handler == libc::SIG_IGN => PreviousDisposition::Ignored,
        _ => PreviousDisposition::Handler,
    }
}

// ---------------------------------------------------------------------------
// The safe-point half (runs on the Lisp thread, where anything is allowed)
// ---------------------------------------------------------------------------

/// GNU's `pending_signals` (src/keyboard.c:105), read by `maybe_quit`
/// (src/lisp.h:3896-3900).  One relaxed `'static` load of the evaluator's
/// asynchronous-attention word, whose `OsSignal` bit it is.
#[inline(always)]
#[doc(hidden)]
pub fn pending() -> bool {
    neovm_eval::emacs_core::eval::ASYNC_ATTENTION
        .is_raised(neovm_eval::emacs_core::eval::AsyncSource::OsSignal)
}

/// Consume a captured fatal-signal request only on the evaluator thread.
#[doc(hidden)]
pub fn take_termination_signal() -> Option<i32> {
    #[cfg(unix)]
    {
        // Latch shutdown for the process lifetime: further signals arriving
        // inside kill-emacs-hook cannot recursively run the hook again.
        let sig = TERMINATION_SIGNAL.load(Ordering::Acquire);
        if sig > 0 {
            TERMINATION_SIGNAL
                .compare_exchange(sig, -1, Ordering::AcqRel, Ordering::Relaxed)
                .ok()
        } else {
            None
        }
    }
    #[cfg(not(unix))]
    None
}

/// Wake bytes coalesce; pending atomics remain the delivery authority.
#[cfg(unix)]
#[doc(hidden)]
pub fn drain_wake_pipe(fd: libc::c_int) {
    let mut bytes = [0u8; 128];
    // SAFETY: fd is the process-lifetime nonblocking pipe's read end.
    while unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) } > 0 {}
}

/// The pending count for one signal, without consuming it.
///
/// Exists because a delivery can land on a thread other than the one that
/// asked for it: `kill (getpid (), sig)` only promises delivery before it
/// returns when the kernel picks the CALLING thread, and it is free not to.
/// That is the property this module is built around -- the handler is correct
/// wherever it lands -- so the tests observe it rather than sidestepping it
/// with `raise`.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn pending_count(signal: HandledSignal) -> u32 {
    PENDING[signal as usize].load(Ordering::Acquire)
}

/// GNU's `store_user_signal_events` drain (src/keyboard.c:8546-8570): take the
/// pending counts and reset them, so a delivery is consumed exactly once.
///
/// `#[cfg(any(test, feature = "test-support"))]` on purpose, and the reason is the entry's finding rather
/// than an oversight: production does NOT take the counts wholesale.
/// [`drain_pending_os_signals`] consumes only the deliveries whose action is
/// the debugger arm and leaves the rest in `p->npending`, because the port of
/// `store_user_signal_events` -- the half that would queue a
/// `USER_SIGNAL_EVENT` for `special-event-map` -- is ledger 184's declared
/// residual.  When that lands, this becomes its drain and the attribute goes.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn take_pending() -> [u32; HandledSignal::COUNT] {
    neovm_eval::emacs_core::eval::ASYNC_ATTENTION
        .take(neovm_eval::emacs_core::eval::AsyncSource::OsSignal);
    let mut taken = [0u32; HandledSignal::COUNT];
    for &signal in supported_signals() {
        taken[signal as usize] = PENDING[signal as usize].swap(0, Ordering::AcqRel);
    }
    taken
}

/// GNU's `handle_user_signal` body (src/keyboard.c:8487-8521), as a decision
/// over data.
///
/// GNU makes this decision *in the handler*, which it can do only because
/// `deliver_process_signal` already forwarded the signal to the thread that
/// owns `Vdebug_on_event`.  Here it is made at the safe point instead, for the
/// reason in the module docs, and the comparison is GNU's: the printed name of
/// `debug-on-event` against the signal's `add_user_signal` NAME.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum UserSignalAction {
    /// `debug-on-event` names this signal: arm the debugger four ways and EAT
    /// the event (src/keyboard.c:8498-8508).
    EnterDebugger,
    /// Otherwise `p->npending++`, and `store_user_signal_events` later queues
    /// a `USER_SIGNAL_EVENT` whose Lisp form is `(intern NAME)`
    /// (src/keyboard.c:7251-7258) for `special-event-map`.
    QueueEvent { lisp_name: &'static str },
}

impl UserSignalAction {
    /// `debug_on_event_name` is `Some` only when `debug-on-event` holds a
    /// SYMBOL, which is GNU's `if (SYMBOLP (Vdebug_on_event))` at :8492.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn for_signal(signal: HandledSignal, debug_on_event_name: Option<&str>) -> Self {
        let disposition = signal.disposition();
        match disposition.drain_site() {
            SignalDrainSite::MaybeQuit => match disposition {
                InstalledDisposition::UserSignal { lisp_name } => match debug_on_event_name {
                    Some(name) if name == lisp_name => Self::EnterDebugger,
                    _ => Self::QueueEvent { lisp_name },
                },
            },
        }
    }
}

/// How many deliveries each arm of [`UserSignalAction`] consumed.
///
/// An engagement counter, not a convenience: ledger P5.2's skip was 100% green
/// and fired ZERO times, so a mechanism that can silently never run has to be
/// able to say how often it ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub struct UserSignalDrain {
    /// Deliveries that took GNU's debugger arm (src/keyboard.c:8498-8508).
    pub(crate) armed_debugger: u32,
    /// Deliveries left in `p->npending` for the input path to queue.
    pub(crate) left_pending: u32,
}

/// GNU's `handle_user_signal` body (src/keyboard.c:8487-8521), run at the Lisp
/// thread's safe point.
///
/// GNU reaches it from `probably_quit`'s `else if (pending_signals)`
/// (src/eval.c:1868-1876), and clears the flag first
/// (`process_pending_signals`, src/keyboard.c:8367-8372) -- both reproduced
/// here, so a delivery cannot make every later safe point take the cold path.
///
/// **Only the debugger arm is consumed.**  GNU "eats the event" there (:8506)
/// and never bumps `npending`; a `QueueEvent` delivery stays in the counter --
/// which IS GNU's `p->npending` storage, not a second one -- until
/// `store_user_signal_events` (:8546-8570) takes it while reading input.  That
/// half is ledger 184's declared residual, and leaving the count in place is
/// what keeps it a residual rather than a lost event.
#[doc(hidden)]
pub fn drain_pending_os_signals(
    eval: &mut neovm_eval::emacs_core::eval::Context,
) -> Result<UserSignalDrain, neovm_value::error::Flow> {
    // GNU's `process_pending_signals` opens with `pending_signals = false;`.
    neovm_eval::emacs_core::eval::ASYNC_ATTENTION
        .take(neovm_eval::emacs_core::eval::AsyncSource::OsSignal);
    // Recheck after the acquiring attention drain: a fatal signal may have
    // arrived between maybe_quit's first check and clearing this attention bit.
    // Never clear its only wake without consuming the published request.
    if let Some(sig) = take_termination_signal()
        && eval.shutdown_request().is_none()
        && match eval.shutdown_phase {
            // `shut_down_emacs` has not started.
            neovm_eval::emacs_core::eval::ShutdownPhase::Running => true,
            // GNU `terminate_due_to_signal` calls `Fkill_emacs` for
            // SIGHUP, SIGTERM, and SIGINT with the first call still on
            // the stack (`emacs.c:453-460`).
            neovm_eval::emacs_core::eval::ShutdownPhase::InProgress => true,
            // `Vrun_hooks` is already nil. A later fatal signal may enter
            // `Fkill_emacs` again.
            neovm_eval::emacs_core::eval::ShutdownPhase::Dying => true,
        }
    {
        neovm_eval::emacs_core::builtins::symbols::builtin_kill_emacs(
            eval,
            vec![neovm_value::value::Value::fixnum(i64::from(sig))],
        )?;
    }

    let debug_on_event = eval.debug_on_event_signal_name();
    let mut drain = UserSignalDrain::default();
    for &signal in supported_signals() {
        let slot = &PENDING[signal as usize];
        let pending_here = slot.load(Ordering::Acquire);
        if pending_here == 0 {
            continue;
        }
        match UserSignalAction::for_signal(signal, debug_on_event.as_deref()) {
            UserSignalAction::EnterDebugger => {
                // GNU decides per DELIVERY, inside the handler; this decides
                // per drain window, because the decision needs Lisp state a
                // handler must not read.  The two differ only if
                // `debug-on-event` changes between two deliveries of the same
                // signal inside one window, which no arm of GNU's own code
                // does.
                slot.fetch_sub(pending_here, Ordering::AcqRel);
                eval.arm_debugger_for_debug_on_event();
                drain.armed_debugger += pending_here;
            }
            UserSignalAction::QueueEvent { .. } => {
                drain.left_pending += pending_here;
            }
        }
    }
    Ok(drain)
}

/// Test-support access to the signal-capture state the moved core tests
/// observe (TEST-SUPPORT-DESIGN rule 8; the statics stay private in product
/// builds). The statics and the C handler are unix-only, and so are the callers.
#[cfg(unix)]
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    pub fn termination_signal() -> &'static std::sync::atomic::AtomicI32 {
        &super::TERMINATION_SIGNAL
    }

    pub fn deliver_user_signal(sig: libc::c_int) {
        super::deliver_user_signal(sig);
    }

    /// Address of the real C signal handler, for the sigaction assertion.
    pub fn deliver_user_signal_addr() -> usize {
        super::deliver_user_signal as *const () as usize
    }
}
