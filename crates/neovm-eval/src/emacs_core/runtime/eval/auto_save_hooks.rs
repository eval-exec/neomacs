//! Cold auto-save upcall table (class-B split seam, F23).
//!
//! GNU `shut_down_emacs` runs `Fdo_auto_save (Qt, Qnil)` after process
//! teardown and before `unlock_all_files` (src/emacs.c), and the command
//! loop runs the same primitive at its auto-save boundaries
//! (src/keyboard.c / Fdo_auto_save in src/fileio.c). The body needs native
//! file writes, messages and the ring bell, so it stays in neovm-system;
//! the system crate depends on the evaluator, so the evaluator reaches the
//! one canonical body through this cold table installed by core at
//! construction. Eval never names the system module.
//!
//! Frequency: per kill-emacs and per command-loop auto-save trigger, never
//! on a per-char or per-event path.

use super::*;
use neovm_value::error::EvalResult;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// Run GNU `Fdo_auto_save (NO_MESSAGE, CURRENT)`.
///
/// The caller resolves absent optionals to nil. The result is the primitive's
/// Lisp return value (GNU returns nil); a failure propagates exactly as the
/// canonical subr's `Flow`.
pub(crate) type DoAutoSave = fn(&mut Context, Value, Value) -> EvalResult;

/// The one cold auto-save body.
#[derive(Clone, Copy)]
pub struct AutoSaveHooks {
    do_auto_save: DoAutoSave,
}

impl AutoSaveHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(do_auto_save: DoAutoSave) -> Self {
        Self { do_auto_save }
    }

    /// The installed `do-auto-save` body.
    pub(crate) fn do_auto_save(&self) -> DoAutoSave {
        self.do_auto_save
    }
}

static AUTO_SAVE_HOOKS: OnceLock<AutoSaveHooks> = OnceLock::new();
static AUTO_SAVE_HOOKS_IDENTITY: OnceLock<&'static AutoSaveHooks> = OnceLock::new();

/// Publish the system layer's auto-save body.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry.
#[must_use = "handle a conflicting auto-save publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_auto_save_hooks(
    hooks: &'static AutoSaveHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&AUTO_SAVE_HOOKS, &AUTO_SAVE_HOOKS_IDENTITY, hooks)
}

/// The installed table.
///
/// Fail-closed: both call sites run during evaluator lifetime, so a missing
/// table means the construction seam itself is broken.
fn auto_save_hooks() -> &'static AutoSaveHooks {
    AUTO_SAVE_HOOKS
        .get()
        .expect("auto-save hooks install at Context construction")
}

/// Run GNU's canonical `Fdo_auto_save` through the installed table.
pub(crate) fn do_auto_save(ctx: &mut Context, no_message: Value, current: Value) -> EvalResult {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(body) = test_body_override::current_thread_body() {
        return body(ctx, no_message, current);
    }
    (auto_save_hooks().do_auto_save())(ctx, no_message, current)
}

/// Test-support observation seam for the auto-save upcall.
///
/// The installed table is a process-wide publish-once descriptor, so a test
/// cannot swap it. A test that must observe the shutdown auto-save installs
/// a body for its own thread only; the guard removes it on drop. The list is
/// a process static keyed by `ThreadId` (no new thread-local), so tests on
/// other threads keep the canonical body. Only the
/// `test_support::authority` fence exports it; it does not exist in a
/// production build.
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_body_override {
    use super::DoAutoSave;
    use std::sync::Mutex;
    use std::thread::ThreadId;

    static THREAD_BODIES: Mutex<Vec<(ThreadId, DoAutoSave)>> = Mutex::new(Vec::new());

    fn bodies() -> std::sync::MutexGuard<'static, Vec<(ThreadId, DoAutoSave)>> {
        THREAD_BODIES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The innermost body installed by the calling thread, if any.
    pub(crate) fn current_thread_body() -> Option<DoAutoSave> {
        let thread = std::thread::current().id();
        bodies()
            .iter()
            .rev()
            .find(|(owner, _)| *owner == thread)
            .map(|(_, body)| *body)
    }

    /// Removes the calling thread's innermost override when dropped.
    pub(crate) struct ThreadBodyGuard {
        thread: ThreadId,
    }

    impl Drop for ThreadBodyGuard {
        fn drop(&mut self) {
            let mut bodies = bodies();
            if let Some(index) = bodies.iter().rposition(|(owner, _)| *owner == self.thread) {
                bodies.remove(index);
            }
        }
    }

    /// Route the calling thread's auto-save upcalls to `body` until the
    /// guard drops.
    pub(crate) fn install_for_current_thread(body: DoAutoSave) -> ThreadBodyGuard {
        let thread = std::thread::current().id();
        bodies().push((thread, body));
        ThreadBodyGuard { thread }
    }
}
