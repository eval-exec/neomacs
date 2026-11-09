//! Cold buffer-kill notification table (stage-4 W4/E12).
//!
//! `kill-buffer` is a core command body. When green threads are running it
//! notifies the evaluator before the buffer dies so threads parked in that
//! buffer move to GNU's `(other-buffer BUFFER)`. The other-buffer resolution
//! binds the frame/window authority and stays in the core command layer
//! (`commands/buffer_ops`); the evaluator reaches it through this one cold
//! PC2 table installed by core at `Context::new_inner` and
//! `Context::from_dump`.
//!
//! Frequency: per buffer kill (never on a per-char or per-event path).

use super::*;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// Resolve GNU's `(other-buffer BUFFER)` for a dying buffer.
///
/// Arguments are the evaluator context, the dying buffer's id, and the dying
/// buffer as the exact Lisp object `kill-buffer` received. The result is the
/// replacement buffer id, or `None` when the upper layer finds none (the
/// threads then keep their buffer, matching GNU).
pub type BeforeBufferKilled = fn(
    &mut Context,
    neovm_edit::buffer::BufferId,
    Value,
) -> Result<Option<neovm_edit::buffer::BufferId>, Flow>;

/// The one cold buffer-kill notification body.
#[derive(Clone, Copy)]
pub struct BufferKillHooks {
    before: BeforeBufferKilled,
}

impl BufferKillHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(before: BeforeBufferKilled) -> Self {
        Self { before }
    }

    /// The installed `before buffer killed` body.
    pub fn before(&self) -> BeforeBufferKilled {
        self.before
    }
}

static BUFFER_KILL_HOOKS: OnceLock<BufferKillHooks> = OnceLock::new();
static BUFFER_KILL_HOOKS_IDENTITY: OnceLock<&'static BufferKillHooks> = OnceLock::new();

/// Publish the core command layer's buffer-kill notification.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting buffer-kill publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_buffer_kill_hooks(
    hooks: &'static BufferKillHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&BUFFER_KILL_HOOKS, &BUFFER_KILL_HOOKS_IDENTITY, hooks)
}

/// The installed table, if core has run one of the two pinned install seams.
pub fn buffer_kill_hooks() -> Option<&'static BufferKillHooks> {
    BUFFER_KILL_HOOKS.get()
}

/// Resolve the replacement buffer for a dying buffer through the cold table.
///
/// Typed refusal if the table is missing: this path is unreachable after
/// construction, so reaching it means the construction seam itself is broken.
pub fn other_buffer_for_killed(
    ctx: &mut Context,
    killed: neovm_edit::buffer::BufferId,
    killed_value: Value,
) -> Result<Option<neovm_edit::buffer::BufferId>, Flow> {
    let Some(hooks) = buffer_kill_hooks() else {
        return Err(signal(
            "error",
            vec![Value::string("buffer-kill hooks are not installed")],
        ));
    };
    (hooks.before())(ctx, killed, killed_value)
}
