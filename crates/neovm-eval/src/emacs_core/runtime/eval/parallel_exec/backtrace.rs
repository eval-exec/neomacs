//! Read-only backtrace adapter and stopped-owner `debug-on-exit` mutation for
//! parallel VM owners (E4-DESIGN §5 :551-561, §8.4a; VM-EXECUTOR-DESIGN V6).
//!
//! While a serial escape runs (or an owner is stopped at an E2 safepoint),
//! [`OwnerBacktraceView`] renders the owner's published [`FrameArena`] frames
//! in GNU innermost-first order (`backtrace-frame` / `backtrace-frames` /
//! `mapbacktrace`, `lisp/native/misc/mod.rs:917-1025`). The view owns its
//! snapshot (`'static`), holds no borrow of `FrameArena` or `VmExec`, is
//! strictly read-only, and is torn down when the escape ticket returns.
//!
//! Setting or clearing `ExitFlags::DEBUG_ON_EXIT` on an owner `VmFrame`
//! (`backtrace-debug`, `src/eval.c:4016-4029`) is valid ONLY while the owner is
//! stopped, enforced by [`StoppedOwnerBacktrace`]: constructed either from a
//! genuine E2 [`AllStopped`] proof, from a checked [`OwnerStopState::Stopped`],
//! or by `VmExec::escape` while parked at the escape boundary.

#![forbid(unsafe_code)]

use super::frames::{ExitFlags, FrameArena, FrameId, FrameKind, OwnedFrame, Pc};
use neovm_value::tagged::gc::AllStopped;
use neovm_value::value::Value;

/// One rendered owner frame inside [`OwnerBacktraceView`].
///
/// Snapshot of a published [`FrameArena`] entry at capture time: holds owned
/// data and no reference to the arena or executor, so it cannot alias or
/// observe a running owner.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OwnerBacktraceEntry {
    pub frame_id: FrameId,
    pub kind: FrameKind,
    /// `true` for evaluated-argument frames (bytecode `VmFrame`, matching GNU
    /// `nargs != UNEVALLED` in `backtrace_frame_apply`, `src/eval.c:3993-3998`);
    /// `false` for unevalled special-form interpreter frames.
    pub evald: bool,
    pub function: Value,
    /// Snapshot of the frame's published argument/operand values at capture
    /// time. For `VmFrame`, this is the live operand segment; for
    /// `InterpFrame`, the unevalled body form when non-nil (single element,
    /// matching `BacktraceFrameSnapshot` in `lisp/native/misc/mod.rs:842-845`).
    pub args: Vec<Value>,
    pub pc: Option<Pc>,
    pub exit_flags: ExitFlags,
}

impl OwnerBacktraceEntry {
    /// Whether GNU's `debug-on-exit` flag is armed on this frame.
    pub(crate) const fn debug_on_exit(&self) -> bool {
        self.exit_flags.contains(ExitFlags::DEBUG_ON_EXIT)
    }

    /// Render the frame's flags plist matching `runtime_backtrace_frame_flags`
    /// (`lisp/native/misc/mod.rs:940-946`): `(:debug-on-exit t)` or `nil`.
    ///
    /// Call only on a thread with an active Lisp heap / rooted serial turn,
    /// because `Value::list` allocates cons cells when `debug_on_exit` is set.
    pub(crate) fn flags_plist(&self) -> Value {
        if self.debug_on_exit() {
            Value::list(vec![Value::symbol(":debug-on-exit"), Value::T])
        } else {
            Value::NIL
        }
    }

    /// Render `(evald, function, args, flags)` matching GNU
    /// `backtrace_frame_apply` (`src/eval.c:3993-3998`) and Neomacs
    /// `apply_backtrace_callback` (`lisp/native/misc/mod.rs:948-972`).
    ///
    /// Call only on a thread with an active Lisp heap / rooted serial turn.
    pub(crate) fn to_lisp_quad(&self) -> (Value, Value, Value, Value) {
        let (evald, args) = if self.evald {
            (Value::T, Value::list(self.args.clone()))
        } else {
            (Value::NIL, self.args.first().copied().unwrap_or(Value::NIL))
        };
        (evald, self.function, args, self.flags_plist())
    }
}

/// Read-only adapter over an owner's published frames (E4-DESIGN §5 :551-561).
///
/// Built from the published [`FrameArena`] while the owner is stopped and torn
/// down when the escape ticket returns. Holds an owned snapshot with no
/// reference to `FrameArena` or `VmExec`, so it is structurally read-only and
/// never borrows a running owner stack.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OwnerBacktraceView {
    entries: Vec<OwnerBacktraceEntry>,
}

const _: fn() = || {
    fn assert_static<T: 'static>() {}
    assert_static::<OwnerBacktraceView>();
    assert_static::<OwnerBacktraceEntry>();
};

impl OwnerBacktraceView {
    /// Capture a read-only snapshot of every live frame in `arena`, ordered
    /// innermost-first (newest first, matching GNU `mapbacktrace` and
    /// `backtrace-frames`). An empty arena produces an empty view.
    pub(crate) fn from_arena(arena: &FrameArena) -> Self {
        let mut entries = Vec::with_capacity(arena.len());
        arena.for_each_frame_innermost_first(|frame_id, frame, operands| {
            let entry = match frame {
                OwnedFrame::Vm(vm) => OwnerBacktraceEntry {
                    frame_id,
                    kind: FrameKind::Vm,
                    evald: true,
                    function: vm.function,
                    args: operands.to_vec(),
                    pc: Some(vm.pc),
                    exit_flags: vm.exit_flags,
                },
                OwnedFrame::Interp(interp) => OwnerBacktraceEntry {
                    frame_id,
                    kind: FrameKind::Interp,
                    evald: false,
                    function: interp.function,
                    args: if interp.body.is_nil() {
                        Vec::new()
                    } else {
                        vec![interp.body]
                    },
                    pc: None,
                    exit_flags: ExitFlags::EMPTY,
                },
            };
            entries.push(entry);
        });
        Self { entries }
    }

    pub(crate) fn entries(&self) -> &[OwnerBacktraceEntry] {
        &self.entries
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Frame at 0-based `level` (0 = innermost owner frame, matching
    /// `backtrace-frame--internal`).
    pub(crate) fn frame_at(&self, level: usize) -> Option<&OwnerBacktraceEntry> {
        self.entries.get(level)
    }

    /// Iterate only the owner's bytecode `VmFrame` entries, innermost-first.
    pub(crate) fn vm_frames(&self) -> impl Iterator<Item = &OwnerBacktraceEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.kind == FrameKind::Vm)
    }

    /// Visit each owner frame in GNU innermost-first order (`mapbacktrace`).
    pub(crate) fn map_frames<E>(
        &self,
        mut visit: impl FnMut(&OwnerBacktraceEntry) -> Result<(), E>,
    ) -> Result<(), E> {
        for entry in &self.entries {
            visit(entry)?;
        }
        Ok(())
    }

    /// Combine the serial escape's own frames (innermost) with this owner's
    /// frames below them (E4-DESIGN §5 :558). When the owner has no live
    /// frames (§8.4a control), returns only `serial_frames`.
    pub(crate) fn combine_with_serial<T: Clone>(
        &self,
        serial_frames: &[T],
        mut map_owner: impl FnMut(&OwnerBacktraceEntry) -> T,
    ) -> Vec<T> {
        let mut out = Vec::with_capacity(serial_frames.len() + self.entries.len());
        out.extend_from_slice(serial_frames);
        for entry in &self.entries {
            out.push(map_owner(entry));
        }
        out
    }
}

/// One pending `backtrace-debug` flag update recorded during a serial escape
/// turn and applied while the owner is still stopped at the escape boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BacktraceDebugUpdate {
    pub frame: FrameId,
    pub debug_on_exit: bool,
}

/// Whether an owner is currently executing bytecode or stopped (idle, parked
/// at a serial escape, or stopped at an E2 safepoint).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OwnerStopState {
    Running,
    Stopped,
}

/// Why a stopped-owner backtrace mutation was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BacktraceDebugError {
    /// The owner is actively running, not stopped.
    OwnerRunning,
}

/// Scoped stopped-owner backtrace adapter.
///
/// Enforces the V6 contract that `set_debug_on_exit(frame_id, bool)` is valid
/// ONLY while the owner is stopped: every constructor either requires a
/// genuine E2 [`AllStopped`] proof, checks [`OwnerStopState::Stopped`], or is
/// restricted to `VmExec::escape` while parked at the escape boundary.
pub(crate) struct StoppedOwnerBacktrace<'a> {
    arena: &'a mut FrameArena,
}

impl<'a> StoppedOwnerBacktrace<'a> {
    /// Open the adapter under a genuine E2 [`AllStopped`] proof.
    pub(crate) fn from_all_stopped(arena: &'a mut FrameArena, _stopped: &AllStopped<'_>) -> Self {
        Self { arena }
    }

    /// Open the adapter only when `state` is [`OwnerStopState::Stopped`];
    /// refuses a running owner with [`BacktraceDebugError::OwnerRunning`].
    pub(crate) fn try_from_state(
        arena: &'a mut FrameArena,
        state: OwnerStopState,
    ) -> Result<Self, BacktraceDebugError> {
        match state {
            OwnerStopState::Stopped => Ok(Self { arena }),
            OwnerStopState::Running => Err(BacktraceDebugError::OwnerRunning),
        }
    }

    /// Open the adapter while `VmExec::escape` is parked waiting for the
    /// serial escape completion (never callable while stepping bytecode).
    pub(super) fn from_escape_park(arena: &'a mut FrameArena) -> Self {
        Self { arena }
    }

    /// Capture a fresh read-only [`OwnerBacktraceView`] snapshot; the returned
    /// view is owned (`'static`) and releases its borrow of `self` immediately.
    pub(crate) fn view(&self) -> OwnerBacktraceView {
        OwnerBacktraceView::from_arena(self.arena)
    }

    /// Set or clear `ExitFlags::DEBUG_ON_EXIT` on the live `VmFrame` named by
    /// `frame_id` (GNU `Fbacktrace_debug`, `src/eval.c:4016-4029`; Neomacs
    /// `set_backtrace_debug_on_exit`, `apply.rs:1023`).
    ///
    /// Returns `true` if a live `VmFrame` matching `frame_id`'s depth and
    /// generation was updated, and `false` for a stale, wrong-generation,
    /// out-of-range, or non-VM (`InterpFrame`) handle without mutating any
    /// frame.
    pub(crate) fn set_debug_on_exit(&mut self, frame_id: FrameId, enabled: bool) -> bool {
        self.arena.set_vm_debug_on_exit_raw(frame_id, enabled)
    }
}
