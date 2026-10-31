//! GNU `frame-or-buffer-changed-p` (src/dispnew.c:7049).
//!
//! GNU keeps a vector of every frame and its name, then every live buffer
//! whose name does not start with a space together with its read-only and
//! modified state, terminated by `lambda`. A call compares that vector with
//! the current state; on a mismatch it rewrites the vector in place, grows or
//! shrinks it when needed, stores it back into VARIABLE (or the internal
//! `frame_and_buffer_state` when VARIABLE is nil), and returns t.
//!
//! The entry order is one fact, so it lives in [`StateEntry`] and both the
//! comparison and the recording walk the same list. Keeping it in one place
//! prevents the two halves of the protocol from drifting.

use crate::buffer::{Buffer, BufferId};
use crate::emacs_core::builtins::symbols::{builtin_set_2, builtin_symbol_value_1};
use crate::emacs_core::error::{EvalResult, expect_args_range};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::{Context, Value};
use crate::window::FrameId;

/// One recorded piece of GNU's state vector.
#[derive(Clone, Copy, Debug)]
enum StateEntry {
    /// `EXPR` is the frame object; `name` follows it in the vector.
    Frame { id: FrameId, name: Value },
    /// `EXPR` is the buffer object, then `read_only` and `modified` follow.
    /// `modified` is exactly `buffer-modified-p`'s value, including the
    /// `autosaved` symbol.
    Buffer {
        id: BufferId,
        read_only: Value,
        modified: Value,
    },
}

impl StateEntry {
    fn matches(self, slots: &[Value], index: &mut usize) -> bool {
        match self {
            Self::Frame { id, name } => {
                same(slots, index, Value::make_frame(id.0)) && same(slots, index, name)
            }
            Self::Buffer {
                id,
                read_only,
                modified,
            } => {
                same(slots, index, Value::make_buffer(id))
                    && same(slots, index, read_only)
                    && same(slots, index, modified)
            }
        }
    }

    fn write(self, state: Value, index: &mut usize) {
        match self {
            Self::Frame { id, name } => {
                set_slot(state, index, Value::make_frame(id.0));
                set_slot(state, index, name);
            }
            Self::Buffer {
                id,
                read_only,
                modified,
            } => {
                set_slot(state, index, Value::make_buffer(id));
                set_slot(state, index, read_only);
                set_slot(state, index, modified);
            }
        }
    }
}

/// A snapshot of the state GNU records, built without running Lisp.
struct FrameBufferState {
    entries: Vec<StateEntry>,
    /// GNU's `n`: `1 + 2 * frames + 3 * live buffers`. Hidden buffers are
    /// counted here even though they are not recorded; the resulting headroom
    /// is part of GNU's vector-size behavior.
    required_without_headroom: usize,
}

impl FrameBufferState {
    fn capture(eval: &Context) -> Self {
        let frames = eval.frames.frame_list();
        let buffers = eval.buffers.buffer_list();
        let mut entries = Vec::with_capacity(frames.len() * 2 + buffers.len() * 3);
        for frame_id in &frames {
            if let Some(frame) = eval.frames.get(*frame_id) {
                entries.push(StateEntry::Frame {
                    id: *frame_id,
                    name: frame.name_value(),
                });
            }
        }
        for buffer_id in &buffers {
            let Some(buffer) = eval.buffers.get(*buffer_id) else {
                continue;
            };
            if buffer_name_is_hidden(buffer) {
                continue;
            }
            entries.push(StateEntry::Buffer {
                id: *buffer_id,
                read_only: buffer.read_only_value(),
                modified: buffer.modified_state_value(),
            });
        }
        Self {
            entries,
            required_without_headroom: 1 + 2 * frames.len() + 3 * buffers.len(),
        }
    }

    fn matches(&self, slots: &[Value]) -> bool {
        let mut index = 0;
        for entry in &self.entries {
            if !entry.matches(slots, &mut index) {
                return false;
            }
        }
        // GNU requires at least one trailing `lambda`; a vector that ends
        // exactly at the entries has no terminator and is treated as changed.
        slots
            .get(index)
            .is_some_and(|slot| slot.bits() == lambda().bits())
    }

    /// GNU reallocates when `n > ASIZE (state)` or
    /// `n + 20 < ASIZE (state) / 2` (src/dispnew.c:7123-7127); otherwise it
    /// rewrites the existing vector in place. `n` counts the terminating
    /// `lambda` and every live buffer, so entries never fill more than
    /// `n - 1` slots and `n == size` still leaves room for the terminator:
    /// GNU reuses that exact-size vector and so must this implementation.
    fn reusable(&self, state: Value) -> bool {
        state.as_vector_data().is_some_and(|slots| {
            let size = slots.len();
            self.required_without_headroom <= size
                && self.required_without_headroom + 20 >= size / 2
        })
    }

    fn write(&self, state: Value, size: usize) {
        let mut index = 0;
        for entry in &self.entries {
            entry.write(state, &mut index);
        }
        while index < size {
            set_slot(state, &mut index, lambda());
        }
    }
}

fn lambda() -> Value {
    Value::symbol("lambda")
}

/// Identity comparison: GNU uses `BASE_EQ` for the frame/buffer objects and
/// `EQ` for their names and state values. `Value`'s `PartialEq` is structural
/// (`equal_value`), so the state vector must compare tagged identities.
fn same(slots: &[Value], index: &mut usize, expected: Value) -> bool {
    let Some(slot) = slots.get(*index) else {
        return false;
    };
    *index += 1;
    slot.bits() == expected.bits()
}

fn set_slot(state: Value, index: &mut usize, value: Value) {
    // The store must happen in release builds too: `debug_assert!` drops its
    // argument expression entirely, so a store inside it would leave the
    // freshly allocated vector filled with `lambda` in production while every
    // debug test passed.
    let written = state.set_vector_slot(*index, value);
    debug_assert!(
        written,
        "frame-or-buffer-changed-p state vector slot must exist"
    );
    let _ = written;
    *index += 1;
}

/// GNU ignores buffers whose names start with a space because they are not
/// included in buffer lists.
fn buffer_name_is_hidden(buffer: &Buffer) -> bool {
    buffer
        .name_value()
        .as_lisp_string()
        .is_some_and(|name| name.as_bytes().first() == Some(&b' '))
}

/// `(frame-or-buffer-changed-p &optional VARIABLE)`.
///
/// VARIABLE is a symbol naming the state vector; nil uses the internal
/// session state. A non-symbol signals `wrong-type-argument`, an unbound
/// symbol signals `void-variable`, and a bound non-vector value is replaced
/// like any other changed state.
pub(crate) fn builtin_frame_or_buffer_changed_p(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-or-buffer-changed-p", &args, 0, 1)?;
    let variable = args.first().copied().unwrap_or(Value::NIL);

    let state = if variable.is_nil() {
        eval.frame_and_buffer_state
    } else {
        Some(builtin_symbol_value_1(eval, variable)?)
    };

    let snapshot = FrameBufferState::capture(eval);
    if let Some(state) = state
        && state
            .as_vector_data()
            .is_some_and(|slots| snapshot.matches(slots))
    {
        return Ok(Value::NIL);
    }

    let state = match state.filter(|state| snapshot.reusable(*state)) {
        Some(state) => {
            let size = state.as_vector_data().map_or(0, |slots| slots.len());
            snapshot.write(state, size);
            state
        }
        None => {
            let size = snapshot.required_without_headroom + 20;
            let state = Value::vector(vec![lambda(); size]);
            snapshot.write(state, size);
            state
        }
    };

    if variable.is_nil() {
        eval.frame_and_buffer_state = Some(state);
    } else {
        // `set` can run variable watchers, which may collect; the freshly
        // built vector is not reachable from Lisp until the set completes.
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(state);
        let result = builtin_set_2(eval, variable, state);
        restore_scratch_gc_roots(saved);
        result?;
    }
    Ok(Value::T)
}

#[cfg(test)]
#[path = "tests/frame_or_buffer_changed_test.rs"]
mod frame_or_buffer_changed_test;
