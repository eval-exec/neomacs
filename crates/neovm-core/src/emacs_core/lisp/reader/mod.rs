//! Reader/printer builtins: read-from-string, read, prin1-to-string (enhanced),
//! format-spec, and various interactive-input stubs.

use super::error::{EvalResult, Flow, FlowKind, FlowResultExt, signal};
#[cfg(test)]
use super::intern::resolve_sym;
use super::intern::{SymId, intern};
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_min_args;
use crate::emacs_core::eval::VariableView;
use crate::emacs_core::symbol::Obarray;
use crate::tagged::SymbolMode;
// storage imports removed — now using emacs_char directly
use super::value::*;
use super::value_reader::SymbolPositionMode;
use crate::buffer::{EmacsBytePos, EmacsByteRange, LispCharPos1};
use std::time::Duration;

pub(crate) mod minibuffer_redisplay;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) struct MinibufferHistorySpec {
    pub(crate) variable_value: Value,
    pub(crate) history_name: Option<SymId>,
    pub(crate) position: Value,
}

fn default_minibuffer_history_spec() -> MinibufferHistorySpec {
    let default_history = intern("minibuffer-history");
    MinibufferHistorySpec {
        variable_value: Value::from_sym_id(default_history),
        history_name: Some(default_history),
        position: Value::fixnum(0),
    }
}

fn normalize_minibuffer_history_position(position: Value) -> Value {
    if position.is_nil() {
        Value::fixnum(0)
    } else {
        position
    }
}

pub(crate) fn minibuffer_history_spec(hist_arg: Option<&Value>) -> MinibufferHistorySpec {
    let Some(hist) = hist_arg.copied() else {
        return default_minibuffer_history_spec();
    };

    match hist.kind() {
        ValueKind::Nil => default_minibuffer_history_spec(),
        ValueKind::Symbol(_id) if hist == Value::T => MinibufferHistorySpec {
            variable_value: Value::T,
            history_name: None,
            position: Value::fixnum(0),
        },
        ValueKind::Symbol(id) => MinibufferHistorySpec {
            variable_value: Value::from_sym_id(id),
            history_name: Some(id),
            position: Value::fixnum(0),
        },
        ValueKind::Cons => {
            let history_var = hist.cons_car();
            let position = normalize_minibuffer_history_position(hist.cons_cdr());
            match history_var.kind() {
                ValueKind::Nil => MinibufferHistorySpec {
                    position,
                    ..default_minibuffer_history_spec()
                },
                ValueKind::Symbol(_id) if history_var == Value::T => MinibufferHistorySpec {
                    variable_value: Value::T,
                    history_name: None,
                    position,
                },
                ValueKind::Symbol(id) => MinibufferHistorySpec {
                    variable_value: Value::from_sym_id(id),
                    history_name: Some(id),
                    position,
                },
                _ => default_minibuffer_history_spec(),
            }
        }
        _ => default_minibuffer_history_spec(),
    }
}

pub(crate) fn initialize_unbound_minibuffer_history(
    shared: &mut super::eval::Context,
    history: MinibufferHistorySpec,
) -> Result<(), Flow> {
    let Some(history_name) = history.history_name else {
        return Ok(());
    };
    if shared
        .visible_runtime_variable_value_by_id(history_name)?
        .is_none()
    {
        shared.try_set_runtime_binding_by_id(history_name, Value::NIL)?;
    }
    Ok(())
}

fn minibuffer_history_limit(vars: VariableView<'_>, history_name: SymId) -> Option<usize> {
    let obarray = vars.obarray();
    let configured = obarray
        .get_property_id(history_name, intern("history-length"))
        .or_else(|| vars.value_by_name("history-length"));

    match configured {
        Some(value) if value == Value::T => None,
        Some(value) if value.is_fixnum() => {
            let limit = value.xfixnum();
            if limit <= 0 {
                Some(0)
            } else {
                Some(limit as usize)
            }
        }
        Some(_) => None,
        None => Some(100),
    }
}

fn add_to_minibuffer_history_variable(
    vars: &mut crate::emacs_core::eval::VariableStore<'_>,
    history_name: SymId,
    value: &crate::heap_types::LispString,
) {
    if value.as_bytes().is_empty() {
        return;
    }

    let new_value = Value::heap_string(value.clone());
    let current = vars.view().value_or_nil(history_name);
    let mut history_items = if current.is_nil() {
        Vec::new()
    } else if let Some(items) = list_to_vec(&current) {
        items
    } else {
        return;
    };

    if history_items.first().copied() == Some(new_value) {
        return;
    }

    if vars
        .view()
        .value_by_name("history-delete-duplicates")
        .is_some_and(|value| value.is_truthy())
    {
        history_items.retain(|entry| *entry != new_value);
    }

    history_items.insert(0, new_value);

    match minibuffer_history_limit(vars.view(), history_name) {
        Some(0) => history_items.clear(),
        Some(max) if history_items.len() > max => history_items.truncate(max),
        _ => {}
    }

    vars.store(history_name, Value::list(history_items));
}

pub(crate) fn history_add_new_input_enabled(vars: VariableView<'_>) -> bool {
    vars.value_by_name("history-add-new-input")
        .is_none_or(|value| value.is_truthy())
}

fn default_minibuffer_string(default: Value) -> Option<crate::heap_types::LispString> {
    if let Some(string) = default.as_lisp_string() {
        return Some(string.clone());
    }
    if default.is_cons() {
        return default.cons_car().as_lisp_string().cloned();
    }
    None
}

pub(crate) fn minibuffer_history_entry(
    result: &crate::heap_types::LispString,
    default: Value,
) -> Option<crate::heap_types::LispString> {
    if result.as_bytes().is_empty() {
        default_minibuffer_string(default)
    } else {
        Some(result.clone())
    }
}

pub(crate) fn add_minibuffer_history_after_unwind(
    eval: &mut super::eval::Context,
    history_name: SymId,
    entry: &crate::heap_types::LispString,
) -> EvalResult {
    let value = Value::heap_string(entry.clone());
    if eval.obarray.fboundp("add-to-history") {
        // Call the GNU Lisp helper only after the caller buffer and its local
        // bindings have been restored.  Raw obarray assignment cannot express
        // a buffer-local history variable.
        eval.apply(
            Value::symbol("add-to-history"),
            vec![Value::from_sym_id(history_name), value],
        )?;
    } else {
        // Interactive reads cannot normally precede subr.el, but retain a
        // bootstrap-safe fallback for deliberately minimal test contexts.
        // SAFETY: the bootstrap helper only reads/stores the history value;
        // it does not call Lisp, alter dispatch metadata, or replace storage.
        let owner = &mut eval.owner;
        unsafe {
            eval.obarray.with_legacy_value_write(|obarray| {
                let mut vars = crate::emacs_core::eval::VariableStore::new(obarray, Some(owner));
                add_to_minibuffer_history_variable(&mut vars, history_name, entry);
            });
        }
    }

    if !entry.as_bytes().is_empty() {
        let max_length =
            minibuffer_history_limit(eval.variables(), history_name).unwrap_or(usize::MAX);
        eval.minibuffers
            .add_to_history_lisp(history_name, entry.clone(), max_length);
    }
    Ok(Value::NIL)
}

pub(crate) fn finish_minibuffer_result_after_unwind(
    eval: &mut super::eval::Context,
    result: crate::heap_types::LispString,
    read: Value,
    default: Value,
) -> EvalResult {
    if read.is_nil() {
        // GNU returns the empty string here.  DEFAULT only supplies history
        // and an input source when READ is non-nil.
        return Ok(Value::heap_string(result));
    }

    let input = if result.as_bytes().is_empty() {
        default_minibuffer_string(default).unwrap_or(result)
    } else {
        result
    };
    let read_result =
        read_from_string_impl(eval.variables(), vec![Value::heap_string(input.clone())])?;
    let value = read_result.cons_car();
    let end_char = read_result.cons_cdr().xfixnum().max(0) as usize;
    let end_byte = if input.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(input.as_bytes(), end_char)
    } else {
        end_char.min(input.as_bytes().len())
    };
    if input.as_bytes()[end_byte..]
        .iter()
        .any(|byte| !matches!(byte, b' ' | b'\t' | b'\n'))
    {
        return Err(signal(
            LispCondition::InvalidReadSyntax,
            vec![Value::string("Trailing garbage following expression")],
        ));
    }
    eval.obarray_mut().materialize_read_symbols(value);
    Ok(value)
}

pub(crate) fn expect_lisp_string(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

/// Frame that invoked the minibuffer.  Kept distinct from the frame that owns
/// its minibuffer window so those roles cannot be swapped at a call site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CallingFrame(pub(crate) neovm_value::display_vocab::FrameId);

/// Frame whose window tree physically contains the active minibuffer window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MinibufferOwnerFrame(neovm_value::display_vocab::FrameId);

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveMinibufferWindowState {
    minibuffer_frame: MinibufferOwnerFrame,
    minibuffer_window_id: neovm_edit::emacs_core::display_metrics::WindowId,
    calling_frame: CallingFrame,
    calling_selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
    previous_minibuffer_frame_selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
    previous_minibuffer_buffer: Option<crate::buffer::BufferId>,
    previous_minibuffer_window_start: LispCharPos1,
    previous_minibuffer_point: LispCharPos1,
    previous_minibuffer_selected_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    previous_active_minibuffer_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
}

/// Display-side effect produced when GNU's `minibuffer_unwind` restores the
/// inactive minibuffer buffer.
///
/// The window-tree mutation is deliberately performed through split borrows
/// of `FrameManager` and `BufferManager`, where the evaluator's redisplay
/// generations are unavailable.  Returning a typed, must-use effect prevents
/// that lower-level mutation from silently dropping the corresponding
/// `wset_update_mode_line` / `wset_redisplay` event.
#[must_use = "a restored minibuffer window must invalidate its display chrome"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MinibufferWindowRestoreEffect {
    NoBufferRestored,
    BufferRestored(neovm_edit::emacs_core::display_metrics::WindowId),
}

impl MinibufferWindowRestoreEffect {
    fn apply(self, eval: &mut super::eval::Context) {
        if let Self::BufferRestored(window) = self {
            // GNU `set_window_buffer` calls `wset_update_mode_line`; after
            // `read_minibuf_unwind` has restored the caller's selection this
            // mini-window is normally nonselected, so `wset_redisplay` also
            // raises `windows_or_buffers_changed` and rebuilds the menu bar.
            eval.gnu_mark_window_mode_line(window);
            eval.mark_chrome_dirty_window(window);
        }
    }
}

/// Structural teardown and the display invalidation it obligates its caller
/// to apply after releasing the split frame/buffer borrows.
#[must_use = "minibuffer teardown effects must be applied before returning"]
struct MinibufferTeardownOutcome {
    inactive_mode_result: EvalResult,
    window_restore: MinibufferWindowRestoreEffect,
}

/// The only two legal updates to GNU's `minibuf_selected_window` when a
/// minibuffer window is activated.
///
/// A recursive read that reuses the already-selected minibuffer must preserve
/// the original caller.  Replacing it with the minibuffer window would make
/// the real caller's mode/header line inactive for the duration of the nested
/// read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferSelectedWindowUpdate {
    Preserve,
    Replace(neovm_edit::emacs_core::display_metrics::WindowId),
}

impl MinibufferSelectedWindowUpdate {
    pub(crate) fn for_entry(
        level: neovm_value::minibuffer_state::MinibufferEntryLevel,
        selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
        minibuffer_window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> Self {
        match level {
            neovm_value::minibuffer_state::MinibufferEntryLevel::Outermost => {
                Self::Replace(selected_window)
            }
            neovm_value::minibuffer_state::MinibufferEntryLevel::Recursive
                if selected_window == minibuffer_window =>
            {
                Self::Preserve
            }
            neovm_value::minibuffer_state::MinibufferEntryLevel::Recursive => {
                Self::Replace(selected_window)
            }
        }
    }

    pub(crate) fn apply(
        self,
        target: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    ) {
        if let Self::Replace(window) = self {
            *target = Some(window);
        }
    }
}

/// How the recursive edit completed, recorded before the session unwind runs.
/// `Pending` covers failures in mode/setup hooks before recursive edit starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferExitDisposition {
    Pending,
    Accepted,
    Aborted,
}

/// Complete native state for one GNU `read_minibuf_unwind` equivalent.
///
/// This value lives inside a typed specpdl action, so every fallible operation
/// after minibuffer activation may use `?` without skipping teardown.  Its
/// private fields ensure it can only be assembled at the activation boundary.
#[derive(Clone, Debug)]
pub(crate) struct MinibufferSessionUnwind {
    minibuf_id: crate::buffer::BufferId,
    depth_before_entry: usize,
    active_window_state: Option<ActiveMinibufferWindowState>,
    saved_buffer_id: Option<crate::buffer::BufferId>,
    saved_current_prefix_arg: Value,
    saved_minibuffer_history_variable: Value,
    saved_minibuffer_history_position: Value,
    saved_command_keys: Vec<Value>,
    saved_raw_command_keys: Vec<Value>,
    disposition: MinibufferExitDisposition,
}

impl MinibufferSessionUnwind {
    /// Assemble one read-minibuffer unwind action at the activation boundary.
    /// The disposition starts `Pending`; the read records its outcome through
    /// [`Self::set_disposition`] once the recursive loop returns.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        minibuf_id: crate::buffer::BufferId,
        depth_before_entry: usize,
        active_window_state: Option<ActiveMinibufferWindowState>,
        saved_buffer_id: Option<crate::buffer::BufferId>,
        saved_current_prefix_arg: Value,
        saved_minibuffer_history_variable: Value,
        saved_minibuffer_history_position: Value,
        saved_command_keys: Vec<Value>,
        saved_raw_command_keys: Vec<Value>,
    ) -> Self {
        Self {
            minibuf_id,
            depth_before_entry,
            active_window_state,
            saved_buffer_id,
            saved_current_prefix_arg,
            saved_minibuffer_history_variable,
            saved_minibuffer_history_position,
            saved_command_keys,
            saved_raw_command_keys,
            disposition: MinibufferExitDisposition::Pending,
        }
    }

    pub(crate) fn set_disposition(&mut self, disposition: MinibufferExitDisposition) {
        self.disposition = disposition;
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        visit(self.saved_current_prefix_arg);
        visit(self.saved_minibuffer_history_variable);
        visit(self.saved_minibuffer_history_position);
        for key in self.saved_command_keys.iter().copied() {
            visit(key);
        }
        for key in self.saved_raw_command_keys.iter().copied() {
            visit(key);
        }
    }
}

fn activate_minibuffer_window_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    minibuf_id: crate::buffer::BufferId,
    entry_level: neovm_value::minibuffer_state::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    let calling_frame = CallingFrame(super::window_cmds::ensure_selected_frame_id_in_state(
        frames, buffers,
    ));
    let calling_frame_state = frames.get(calling_frame.0)?;
    let minibuffer_window_id = calling_frame_state.minibuffer_window?;
    let calling_selected_window = calling_frame_state.selected_window;
    let minibuffer_frame = MinibufferOwnerFrame(frames.find_window_frame_id(minibuffer_window_id)?);
    let minibuffer_frame_state = frames.get(minibuffer_frame.0)?;
    let previous_minibuffer_frame_selected_window = minibuffer_frame_state.selected_window;
    let mut previous_minibuffer_buffer = None;
    let mut previous_minibuffer_window_start = LispCharPos1::ONE;
    let mut previous_minibuffer_point = LispCharPos1::ONE;
    if let Some(crate::window::Window::Leaf {
        buffer_id,
        window_start,
        point,
        ..
    }) = minibuffer_frame_state.find_window(minibuffer_window_id)
    {
        previous_minibuffer_buffer = Some(*buffer_id);
        previous_minibuffer_window_start = *window_start;
        previous_minibuffer_point = *point;
    }

    let saved = ActiveMinibufferWindowState {
        minibuffer_frame,
        minibuffer_window_id,
        calling_frame,
        calling_selected_window,
        previous_minibuffer_frame_selected_window,
        previous_minibuffer_buffer,
        previous_minibuffer_window_start,
        previous_minibuffer_point,
        previous_minibuffer_selected_window: *minibuffer_selected_window,
        previous_active_minibuffer_window: *active_minibuffer_window,
    };

    // GNU saves the caller's live BUF_PT into its window point marker before
    // selecting the minibuffer.  The caller becomes non-selected during the
    // read, so redisplay and mode-line `%l` must read that saved point rather
    // than the window's stale construction-time marker.
    super::window_cmds::remember_selected_window_point_in_state(frames, buffers, calling_frame.0);
    if let Some(frame) = frames.get_mut(minibuffer_frame.0) {
        if let Some(window) = frame.find_window_mut(minibuffer_window_id) {
            window.set_buffer(minibuf_id);
            debug_assert_eq!(window.buffer_id(), Some(minibuf_id));
            crate::window::window_markers::attach_window_position_markers(buffers, window);
        }
        let _ = frame.select_window(minibuffer_window_id);
    }
    let _ = frames.select_frame(minibuffer_frame.0);
    buffers.switch_current(minibuf_id);
    MinibufferSelectedWindowUpdate::for_entry(
        entry_level,
        calling_selected_window,
        minibuffer_window_id,
    )
    .apply(minibuffer_selected_window);
    *active_minibuffer_window = Some(minibuffer_window_id);
    Some(saved)
}

/// Record the observable side effects of selecting the active minibuffer.
///
/// GNU `read_minibuf` selects the minibuffer through `Fselect_window` with a
/// nil NORECORD argument (`src/minibuf.c`).  Consequently the minibuffer is
/// placed at the front of its *owner frame's* buffer list, the window use time
/// is updated, and `buffer-list-update-hook` runs.  Activation itself remains
/// an infallible state transition so its unwind action can be installed before
/// this helper executes arbitrary Lisp from the hook.
pub(crate) fn record_active_minibuffer_selection(
    eval: &mut super::eval::Context,
    active: ActiveMinibufferWindowState,
    minibuf_id: crate::buffer::BufferId,
) -> EvalResult {
    let _ = eval
        .frames
        .note_window_selected(active.minibuffer_window_id);
    super::window_cmds::record_buffer_in_state(
        &mut eval.frames,
        &mut eval.edit.buffers,
        minibuf_id,
        active.minibuffer_frame.0,
    )?;
    if !eval.edit.buffers.buffer_hooks_inhibited(minibuf_id) {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

/// Record GNU `read_minibuf_unwind` reselecting the invoking window through
/// `Fset_frame_selected_window(..., NORECORD=nil)`.
fn record_restored_calling_window_selection(
    eval: &mut super::eval::Context,
    active: ActiveMinibufferWindowState,
) -> EvalResult {
    let Some(buffer_id) = eval
        .frames
        .get(active.calling_frame.0)
        .and_then(|frame| frame.find_window(active.calling_selected_window))
        .and_then(crate::window::Window::buffer_id)
    else {
        return Ok(Value::NIL);
    };
    let _ = eval
        .frames
        .note_window_selected(active.calling_selected_window);
    super::window_cmds::record_buffer_in_state(
        &mut eval.frames,
        &mut eval.edit.buffers,
        buffer_id,
        active.calling_frame.0,
    )?;
    if !eval.edit.buffers.buffer_hooks_inhibited(buffer_id) {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn activate_minibuffer_window(
    eval: &mut super::eval::Context,
    minibuf_id: crate::buffer::BufferId,
    entry_level: neovm_value::minibuffer_state::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    if super::eval::gnu_redisplay_hooks_enabled() {
        return minibuffer_redisplay::activate(eval, minibuf_id, entry_level);
    }
    activate_minibuffer_window_in_state(
        &mut eval.frames,
        &mut eval.edit.buffers,
        &mut eval.minibuffer_selected_window,
        &mut eval.active_minibuffer_window,
        minibuf_id,
        entry_level,
    )
}

/// GNU's separate minibuffer_unwind action runs after window configurations.
/// The payload holds only owning Context buffer/window identities, never Values.
/// Each mutator owns a disjoint payload and restores it through an exclusive
/// Context borrow; independent mutators never share this unwind state.
#[derive(Clone, Debug, Default)]
pub(crate) struct MinibufferBufferUnwind {
    saved: Option<ActiveMinibufferWindowState>,
}

impl MinibufferBufferUnwind {
    pub(crate) fn set_saved(&mut self, saved: Option<ActiveMinibufferWindowState>) {
        self.saved = saved;
    }
}

#[cold]
#[inline(never)]
pub(crate) fn unwind_minibuffer_buffer(
    eval: &mut super::eval::Context,
    state: MinibufferBufferUnwind,
) -> EvalResult {
    if let Some(saved) = state.saved {
        minibuffer_redisplay::restore_buffer(eval, saved).apply(eval);
    }
    Ok(Value::NIL)
}

fn restore_minibuffer_window_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    saved: ActiveMinibufferWindowState,
) -> MinibufferWindowRestoreEffect {
    let mut window_restore = MinibufferWindowRestoreEffect::NoBufferRestored;
    if let Some(frame) = frames.get_mut(saved.minibuffer_frame.0) {
        if let Some(window) = frame.find_window_mut(saved.minibuffer_window_id)
            && let Some(prev_buffer_id) = saved.previous_minibuffer_buffer
        {
            window.set_buffer(prev_buffer_id);
            window_restore =
                MinibufferWindowRestoreEffect::BufferRestored(saved.minibuffer_window_id);
            debug_assert_eq!(window.buffer_id(), Some(prev_buffer_id));
            crate::window::window_markers::attach_window_position_markers(buffers, window);
            crate::window::window_markers::set_window_start_with_marker(
                buffers,
                window,
                saved.previous_minibuffer_window_start,
            );
            crate::window::window_markers::set_window_point_with_marker(
                buffers,
                window,
                saved.previous_minibuffer_point,
            );
        }
    }
    restore_minibuffer_selection_in_state(
        frames,
        minibuffer_selected_window,
        active_minibuffer_window,
        saved,
    );
    window_restore
}

fn restore_minibuffer_selection_in_state(
    frames: &mut crate::window::FrameManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    saved: ActiveMinibufferWindowState,
) {
    if let Some(frame) = frames.get_mut(saved.minibuffer_frame.0) {
        let _ = frame.select_window(saved.previous_minibuffer_frame_selected_window);
    }
    if let Some(frame) = frames.get_mut(saved.calling_frame.0) {
        let _ = frame.select_window(saved.calling_selected_window);
    }
    if frames.get(saved.calling_frame.0).is_some()
        && frames
            .selected_frame()
            .is_none_or(|frame| frame.id != saved.calling_frame.0)
    {
        let _ = frames.select_frame(saved.calling_frame.0);
    }
    *minibuffer_selected_window = saved.previous_minibuffer_selected_window;
    *active_minibuffer_window = saved.previous_active_minibuffer_window;
}

fn erase_expired_minibuffer_buffer_in_state(
    buffers: &mut crate::buffer::BufferManager,
    minibuf_id: crate::buffer::BufferId,
) {
    // GNU `read_minibuf_unwind` (minibuf.c:1181) erases the expired buffer's
    // text, and its companion `get_minibuffer` reuse path (minibuf.c:1062-1063)
    // drops the buffer's overlays. neomacs previously erased text only, so a
    // vertico candidate `after-string` overlay anchored on ` *Minibuf-N*`
    // survived teardown and kept the mini-window measuring as multi-line. Delete
    // the overlays here so the expired buffer is fully reset (text + overlays).
    let _ = buffers.delete_all_buffer_overlays(minibuf_id);
    let _ = buffers.replace_buffer_contents(minibuf_id, "");
}

/// Tear down one minibuffer level, mirroring GNU's two-responsibility unwind
/// (`read_minibuf_unwind` + `minibuffer_unwind`, minibuf.c) as a single unit.
///
/// This is the ONLY path through which minibuffer exit and abort flow, so the
/// two are provably identical (GNU runs the same unwind on both — both leave via
/// `(throw 'exit …)`; there is no abort-specific teardown). The steps, in GNU
/// order, are:
///
/// - **R1** Reset the expired ` *Minibuf-N*` completely — delete its overlays
///   *and* erase its text (the vertico candidate `after-string` overlay is the
///   actual carrier of the multi-line content, so text-erase alone is not
///   enough), then run `minibuffer-inactive-mode`.
/// - **R2** Restore the mini-window's buffer to ` *Minibuf-0*` (the saved
///   `previous_minibuffer_buffer`), the analogue of `minibuffer_unwind`.
/// - **R3** At the OUTERMOST level only (`minibuffers.depth() == 0` after the
///   pop, matching GNU's `minibuf_level == 0` guard at minibuf.c:1188), force
///   the mini-window back to exactly one line, content-independent.
/// - **R4** Invalidate the mini-window's cached glyph-matrix row count (folded
///   into `force_resize_mini_window_to_one_line`) so the layout engine cannot
///   reuse the stale 35-row matrix on the next redisplay.
///
/// `depth_after_pop` is `minibuffers.depth()` taken AFTER the exit/abort pop has
/// already run. `run_inactive_mode` runs `minibuffer-inactive-mode` and its
/// result is returned for the caller to `?`-propagate, exactly as before.
#[allow(clippy::too_many_arguments)]
fn teardown_minibuffer_level_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    minibuf_id: crate::buffer::BufferId,
    depth_after_pop: usize,
    saved: ActiveMinibufferWindowState,
    run_inactive_mode: impl FnOnce() -> EvalResult,
) -> MinibufferTeardownOutcome {
    let teardown_frame_id = saved.minibuffer_frame.0;

    // (R1) Completely reset the expired *Minibuf-N* (overlays + text), then run
    // minibuffer-inactive-mode.
    erase_expired_minibuffer_buffer_in_state(buffers, minibuf_id);
    let inactive_mode_result = run_inactive_mode();

    // (R2) Restore the mini-window's buffer to *Minibuf-0* / the prev buffer.
    let window_restore = restore_minibuffer_window_in_state(
        frames,
        buffers,
        minibuffer_selected_window,
        active_minibuffer_window,
        saved,
    );

    // (R3 + R4) At the outermost level, force the mini-window back to one line
    // and invalidate its cached matrix so the engine cannot reuse the stale
    // row count. Guarded by depth==0 so a nested minibuffer popping back to an
    // outer (still active) one does not collapse the outer minibuffer's window.
    if depth_after_pop == 0 {
        frames.force_resize_mini_window_to_one_line(teardown_frame_id);
    }

    MinibufferTeardownOutcome {
        inactive_mode_result,
        window_restore,
    }
}

/// Execute the typed cleanup registered immediately after minibuffer-window
/// activation.  It deliberately stores hook failures until every structural
/// restoration has completed, matching GNU's unwind discipline: an exit hook
/// cannot strand an active minibuffer or skip the caller-window restoration.
pub(crate) fn unwind_minibuffer_session(
    shared: &mut super::eval::Context,
    state: MinibufferSessionUnwind,
) -> EvalResult {
    let restored_calling_selection = state.active_window_state;
    let exit_hook_result = match shared.run_hook_if_bound("minibuffer-exit-hook").kinded() {
        Ok(value) => Ok(value),
        Err(FlowKind::Signal(_)) => Ok(Value::NIL),
        Err(flow) => Err(Flow::from_kind(flow)),
    };

    if shared.minibuffers.depth() > state.depth_before_entry {
        match state.disposition {
            MinibufferExitDisposition::Accepted => {
                let _ = shared.minibuffers.exit_minibuffer();
            }
            MinibufferExitDisposition::Pending | MinibufferExitDisposition::Aborted => {
                shared.minibuffers.abort_minibuffer();
            }
        }
    }
    // Defensive recovery for a future nested setup path: specpdl actions
    // unwind LIFO, so normally the loop executes zero times after the pop
    // above.  The invariant on return is the captured entry depth.
    while shared.minibuffers.depth() > state.depth_before_entry {
        shared.minibuffers.abort_minibuffer();
    }

    let teardown_outcome = if let Some(saved) = state.active_window_state {
        let _ = shared
            .edit
            .buffers
            .switch_current_unrecorded(state.minibuf_id);
        if super::eval::gnu_redisplay_hooks_enabled() {
            let depth_after_pop = shared.minibuffers.depth();
            minibuffer_redisplay::teardown(shared, state.minibuf_id, depth_after_pop, saved)
        } else {
            let shared_ptr = std::ptr::NonNull::from(&mut *shared);
            teardown_minibuffer_level_in_state(
                &mut shared.frames,
                &mut shared.edit.buffers,
                &mut shared.minibuffer_selected_window,
                &mut shared.active_minibuffer_window,
                state.minibuf_id,
                shared.minibuffers.depth(),
                saved,
                move || unsafe {
                    run_minibuffer_mode_if_bound(
                        shared_ptr.as_ptr().as_mut().unwrap(),
                        "minibuffer-inactive-mode",
                    )
                },
            )
        }
    } else {
        minibuffer_redisplay::publish_expired_buffer(shared, state.minibuf_id);
        erase_expired_minibuffer_buffer_in_state(&mut shared.edit.buffers, state.minibuf_id);
        MinibufferTeardownOutcome {
            inactive_mode_result: run_minibuffer_mode_if_bound(shared, "minibuffer-inactive-mode"),
            window_restore: MinibufferWindowRestoreEffect::NoBufferRestored,
        }
    };
    let redirect_result = super::frame::sync_gui_frame_focus_redirects(shared);
    teardown_outcome.window_restore.apply(shared);
    let inactive_mode_result = teardown_outcome.inactive_mode_result;

    if let Some(buffer_id) = state.saved_buffer_id
        && shared.edit.buffers.get(buffer_id).is_some()
    {
        shared.edit.buffers.switch_current(buffer_id);
    }
    // A sizing/inactive-mode transfer skips the remaining read_minibuf_unwind
    // statements. Separate configuration and minibuffer restoration still run.
    // A failed focus-redirect sync likewise skips the selection record.
    let selection_record_result = if redirect_result.is_err()
        || (super::eval::gnu_redisplay_hooks_enabled() && inactive_mode_result.is_err())
    {
        Ok(Value::NIL)
    } else {
        restored_calling_selection
            .map(|active| record_restored_calling_window_selection(shared, active))
            .unwrap_or(Ok(Value::NIL))
    };
    shared.store_variable_by_name(
        "minibuffer-depth",
        Value::fixnum(shared.minibuffers.depth() as i64),
    );
    shared.store_variable_by_name("current-prefix-arg", state.saved_current_prefix_arg);
    shared.store_variable_by_name(
        "minibuffer-history-variable",
        state.saved_minibuffer_history_variable,
    );
    shared.store_variable_by_name(
        "minibuffer-history-position",
        state.saved_minibuffer_history_position,
    );
    shared.set_command_key_sequences(state.saved_command_keys, state.saved_raw_command_keys);

    tracing::debug!(
        "read-from-minibuffer: unwound current_buffer={:?} depth={} active_window={:?} selected_window={:?}",
        shared.edit.buffers.current_buffer_id(),
        shared.minibuffers.depth(),
        shared.active_minibuffer_window,
        shared
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window)
    );

    exit_hook_result?;
    inactive_mode_result?;
    redirect_result?;
    selection_record_result?;
    Ok(Value::NIL)
}

pub(crate) fn find_or_create_minibuffer_buffer_in_state(
    buffers: &mut crate::buffer::BufferManager,
    depth: usize,
) -> crate::buffer::BufferId {
    let minibuf_name = format!(" *Minibuf-{depth}*");
    let minibuf_id = match buffers.find_buffer_by_name(&minibuf_name) {
        Some(existing) => {
            // GNU `get_minibuffer` (minibuf.c:1062-1063) resets every reused
            // minibuffer pool buffer with `delete_all_overlays + reset_buffer`
            // so a new activation never inherits stale overlays or text from a
            // prior (possibly aborted) session. Mirror that defense-in-depth
            // here on the reuse branch: even if a teardown was skipped, the
            // buffer starts clean.
            let _ = buffers.delete_all_buffer_overlays(existing);
            let _ = buffers.replace_buffer_contents(existing, "");
            existing
        }
        None => buffers.create_buffer(&minibuf_name),
    };
    let _ = buffers.configure_buffer_undo_list(minibuf_id, Value::NIL);
    let _ = buffers.set_buffer_local_property(minibuf_id, "truncate-lines", Value::NIL);
    minibuf_id
}

/// Capture the directory that a newly activated minibuffer should inherit.
///
/// GNU `read_minibuf` snapshots `BVAR (current_buffer, directory)` before it
/// switches to `*Minibuf-N*`, then installs that value in the minibuffer after
/// `set_minibuffer_mode`.  The fallback scan is GNU's minibuffer-only-frame
/// defense for callers whose current buffer has no string directory.
pub(crate) fn minibuffer_ambient_directory_in_state(
    buffers: &crate::buffer::BufferManager,
) -> Option<Value> {
    let current = buffers.current_buffer_id();
    current
        .and_then(|id| buffers.get(id))
        .and_then(|buffer| buffer.buffer_local_value("default-directory"))
        .filter(|value| value.is_string())
        .or_else(|| {
            buffers.buffer_list().into_iter().find_map(|id| {
                buffers
                    .get(id)
                    .and_then(|buffer| buffer.buffer_local_value("default-directory"))
                    .filter(|value| value.is_string())
            })
        })
}

pub(crate) fn install_minibuffer_ambient_directory_in_state(
    buffers: &mut crate::buffer::BufferManager,
    minibuf_id: crate::buffer::BufferId,
    ambient_directory: Option<Value>,
) {
    if let Some(directory) = ambient_directory {
        let _ = buffers.set_buffer_local_property(minibuf_id, "default-directory", directory);
    }
}

pub(crate) fn run_minibuffer_mode_if_bound(
    eval: &mut super::eval::Context,
    mode: &str,
) -> EvalResult {
    if eval.obarray().symbol_function(mode).is_some() {
        eval.apply0(Value::symbol(mode))
    } else {
        Ok(Value::NIL)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn restore_minibuffer_window(eval: &mut super::eval::Context, saved: ActiveMinibufferWindowState) {
    if super::eval::gnu_redisplay_hooks_enabled() {
        minibuffer_redisplay::restore(eval, saved).apply(eval);
        return;
    }
    let effect = restore_minibuffer_window_in_state(
        &mut eval.frames,
        &mut eval.edit.buffers,
        &mut eval.minibuffer_selected_window,
        &mut eval.active_minibuffer_window,
        saved,
    );
    effect.apply(eval);
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn signal_invalid_read_syntax_in_lisp_string(
    buffer_text: &crate::heap_types::LispString,
    absolute_error_pos: usize,
    message: String,
) -> Flow {
    let clamped_pos = absolute_error_pos.min(buffer_text.sbytes());
    let prefix = &buffer_text.as_bytes()[..clamped_pos];
    let line = prefix.iter().filter(|&&byte| byte == b'\n').count() as i64 + 1;
    let line_start = prefix
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let column = if buffer_text.is_multibyte() {
        crate::emacs_core::emacs_char::chars_in_multibyte(&prefix[line_start..]) as i64
    } else {
        (prefix.len() - line_start) as i64
    };
    signal(
        LispCondition::InvalidReadSyntax,
        vec![
            Value::string(message),
            Value::fixnum(line),
            Value::fixnum(column),
        ],
    )
}

/// GNU `invalid_syntax_lisp` (`src/lread.c:636-652`) measures the line
/// from BEGV and the column with current_column, including display widths.
/// Point is already published at the reader's stop position before this call.
fn signal_invalid_read_syntax_in_buffer_object(
    ctx: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    absolute_error_pos: EmacsBytePos,
    message: String,
) -> Flow {
    let Some(buffer) = ctx.edit.buffers.get(buffer_id) else {
        return signal(
            LispCondition::Error,
            vec![Value::string("Buffer does not exist")],
        );
    };
    let accessible = buffer.accessible_emacs_byte_region();
    let end = accessible.clamp(absolute_error_pos);
    let range = EmacsByteRange::new(accessible.start(), end);
    let mut prefix = Vec::with_capacity(range.len().get());
    buffer.copy_emacs_byte_range_to(range, &mut prefix);
    let line = prefix.iter().filter(|&&byte| byte == b'\n').count() as i64 + 1;
    match super::indent::display_column_at_emacs_byte_pos(ctx, buffer_id, end) {
        Ok(column) => signal(
            LispCondition::InvalidReadSyntax,
            vec![
                Value::string(message),
                Value::fixnum(line),
                Value::fixnum(column as i64),
            ],
        ),
        Err(flow) => flow,
    }
}

pub(crate) fn end_of_file_error_for_source(source: Option<Value>) -> Flow {
    signal(LispCondition::EndOfFile, source.into_iter().collect())
}

/// Which stream a `readevalloop` is reading, in the only terms GNU's
/// `end_of_file_error` (`src/lread.c:2121-2132`) distinguishes:
///
/// ```c
/// static AVOID
/// end_of_file_error (source_t *source)
/// {
///   if (from_file_p (source))
///     /* Only Fload calls read on a file, and Fload always binds
///        load-true-file-name around the call.  */
///     xsignal1 (Qend_of_file, Vload_true_file_name);
///   else if (from_buffer_p (source))
///     xsignal1 (Qend_of_file, source->object);
///   else
///     xsignal0 (Qend_of_file);
/// }
/// ```
///
/// The datum is a property of the STREAM, not of the site that notices the
/// truncation, so every readevalloop carries one of these and no raise site
/// assembles `end-of-file` data of its own.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ReadSourceObject {
    /// GNU `from_file_p`: `Fload` reading a file directly (a `.elc`, or a
    /// source file when `load-source-file-function` is nil).  The datum is
    /// `load-true-file-name`, which `Fload` binds around the read.
    LoadFile(Value),
    /// GNU `from_buffer_p`: the buffer `readcharfun` names.  `eval-buffer`
    /// and `eval-region` read this way, and so does `load` for a source file
    /// once `load-with-code-conversion` has put the text in a temp buffer —
    /// which is why GNU reports a truncated `.el` as `#<killed buffer>`.
    Buffer(Value),
    /// Strings, markers and reader functions: GNU signals with no datum.
    Anonymous,
}

impl ReadSourceObject {
    /// The datum GNU attaches to reader errors raised on this stream.
    pub(crate) fn error_datum(self) -> Option<Value> {
        match self {
            Self::LoadFile(value) | Self::Buffer(value) => Some(value),
            Self::Anonymous => None,
        }
    }

    /// GNU `end_of_file_error` (`src/lread.c:2121-2132`).
    pub(crate) fn end_of_file_error(self) -> Flow {
        end_of_file_error_for_source(self.error_datum())
    }
}

/// Read the next top-level form from the active load-read cursor — the stream
/// `standard-input` is bound to during a `load`/`eval-buffer` readevalloop
/// (see [`crate::emacs_core::eval::LoadReadStreamToken`]).  Advancing the
/// shared byte cursor makes the enclosing loop resume *after* this form,
/// exactly like GNU's shared `readcharfun` (lread.c `readevalloop`): a file
/// that calls `(read)` mid-load consumes its next top-level form.
pub(crate) fn read_from_active_load_cursor(
    ctx: &mut crate::emacs_core::eval::Context,
    symbol_position_mode: SymbolPositionMode,
) -> EvalResult {
    let Some(cursor) = ctx.load_read_cursors.last() else {
        // `standard-input` names the load stream but no load is active: treat
        // as a spent stream (EOF) rather than crashing.
        return Err(ReadSourceObject::Anonymous.end_of_file_error());
    };
    let source = cursor.source;
    let eof_source = cursor.eof_source;
    let pos = cursor.pos;
    let shorthands = match &cursor.shorthands {
        super::eval::LoadReadShorthands::Snapshot(snapshot) => snapshot.clone(),
        super::eval::LoadReadShorthands::CurrentBinding => current_read_symbol_shorthands(ctx),
    };

    let lisp_str = ctx.lisp_string(source).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("load-read stream source is not a string")],
        )
    })?;
    let read_source = super::value_reader::LispReadSource::new(lisp_str);
    let end = read_source.logical_len();
    if pos >= end {
        return Err(end_of_file_error_for_source(eof_source));
    }
    let read_result = read_source
        .read_one_range_with_locate_syms_with_symbols(
            pos,
            end,
            symbol_position_mode,
            ctx.variables(),
            shorthands.as_ref(),
            ctx.symbol_mode(),
        )
        .map_err(signal_reader_error_from_string)?;
    let Some((value, next_pos)) = read_result else {
        return Err(end_of_file_error_for_source(eof_source));
    };
    // Advance the shared cursor so the readevalloop resumes after this form.
    if let Some(cursor) = ctx.load_read_cursors.last_mut() {
        cursor.pos = next_pos;
    }
    ctx.obarray_mut().materialize_read_symbols(value);
    Ok(value)
}

pub(crate) fn signal_reader_error_from_string(e: super::value_reader::ReadError) -> Flow {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => {
            ReadSourceObject::Anonymous.end_of_file_error()
        }
        super::value_reader::ReadErrorKind::Error => {
            signal(LispCondition::Error, vec![Value::string(e.message)])
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => signal(
            LispCondition::InvalidReadSyntax,
            vec![Value::string(e.message)],
        ),
        super::value_reader::ReadErrorKind::Signal => signal(
            crate::emacs_core::error::UserCondition::from(
                e.signal_symbol.unwrap_or_else(|| intern("error")),
            ),
            e.signal_data,
        ),
    }
}

pub(crate) fn signal_reader_error_from_buffer(
    ctx: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    e: super::value_reader::ReadError,
) -> Flow {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => {
            ReadSourceObject::Buffer(Value::make_buffer(buffer_id)).end_of_file_error()
        }
        super::value_reader::ReadErrorKind::Error => {
            signal(LispCondition::Error, vec![Value::string(e.message)])
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => {
            signal_invalid_read_syntax_in_buffer_object(
                ctx,
                buffer_id,
                EmacsBytePos::new(e.position),
                e.message,
            )
        }
        super::value_reader::ReadErrorKind::Signal => signal(
            crate::emacs_core::error::UserCondition::from(
                e.signal_symbol.unwrap_or_else(|| intern("error")),
            ),
            e.signal_data,
        ),
    }
}

pub(crate) fn stdin_end_of_file_error() -> Flow {
    signal(
        LispCondition::EndOfFile,
        vec![Value::string("Error reading from stdin")],
    )
}

// ---------------------------------------------------------------------------
// 1. read-from-string
// ---------------------------------------------------------------------------

/// `(read-from-string STRING &optional START END)`
///
/// Parse a single Lisp object from STRING starting at position START (default 0).
/// Returns `(OBJECT . END-POSITION)` where END-POSITION is the character index
/// after the parsed object.
/// Fetch the active `read-symbol-shorthands` value and build the reader's
/// shorthand table.  GNU's `read`/`read-from-string` consult this variable
/// (set **buffer-local** by `hack-local-variables` during
/// `byte-compile-file`), so reading source that declares
/// `read-symbol-shorthands` in its local variables rewrites `prefix:name`
/// symbols.  The value must be resolved with buffer-local visibility — the
/// global binding is normally nil — hence the current buffer's binding cell
/// (`read_symbol_shorthands_value`) rather than the raw obarray slot.
/// Returns `None` when unset/nil.
pub(crate) fn current_read_symbol_shorthands(
    eval: &super::eval::Context,
) -> Option<super::value_reader::ReadSymbolShorthands> {
    let value = super::builtins::symbols::read_symbol_shorthands_value(eval);
    if value.is_nil() {
        return None;
    }
    super::value_reader::ReadSymbolShorthands::from_lisp_value(value)
}

pub(crate) fn builtin_read_from_string(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let shorthands = current_read_symbol_shorthands(ctx);
    let result = read_from_string_impl_inner(
        ctx.variables(),
        args,
        SymbolPositionMode::Bare,
        shorthands.as_ref(),
        ctx.symbol_mode(),
    )?;
    if result.is_cons() {
        ctx.obarray_mut()
            .materialize_read_symbols(result.cons_car());
    }
    Ok(result)
}

pub(crate) fn read_from_string_impl(
    vars: crate::emacs_core::eval::VariableView<'_>,
    args: Vec<Value>,
) -> EvalResult {
    read_from_string_impl_inner(vars, args, SymbolPositionMode::Bare, None, SymbolMode::Bare)
}

pub(crate) fn read_from_string_impl_inner(
    vars: VariableView<'_>,
    args: Vec<Value>,
    symbol_position_mode: SymbolPositionMode,
    shorthands: Option<&super::value_reader::ReadSymbolShorthands>,
    mode: SymbolMode,
) -> EvalResult {
    expect_min_args("read-from-string", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("read-from-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let full_string = expect_lisp_string(&args[0])?;
    let read_source = super::value_reader::LispReadSource::new(&full_string);

    // GNU Emacs `Fread_from_string` (`src/lread.c:2514`) treats START and
    // END as character indices into STRING (validated via
    // `validate_subarray` against `SCHARS (string)`), translates them to
    // START's byte offset through `string_char_to_byte`, keeps END logical,
    // and reports
    // FINAL-STRING-INDEX as a *character* index too. Indexing by raw
    // UTF-8 byte length here was a long-standing bug (audit §11.6) that
    // would either panic on multibyte input (slicing mid-codepoint) or
    // return a byte offset where elisp expected a character count.
    let char_count = full_string.schars();

    let start_arg = args.get(1).cloned().unwrap_or(Value::NIL);
    let end_arg = args.get(2).cloned().unwrap_or(Value::NIL);
    let to_char_index = |value: &Value| -> Result<usize, Flow> {
        match value.kind() {
            ValueKind::Nil => Ok(0),
            ValueKind::Fixnum(n) => {
                let idx = if n < 0 { (char_count as i64) + n } else { n };
                if idx < 0 || idx > char_count as i64 {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], start_arg, end_arg],
                    ));
                }
                Ok(idx as usize)
            }
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), *value],
            )),
        }
    };
    let start_char = if args.len() > 1 {
        to_char_index(&start_arg)?
    } else {
        0
    };
    let end_char = if args.len() > 2 {
        to_char_index(&end_arg)?
    } else {
        char_count
    };

    if start_char > end_char {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], start_arg, end_arg],
        ));
    }

    let range = crate::buffer::CharRange::from_start_len(
        crate::buffer::CharPos0::new(start_char),
        crate::buffer::CharLen::new(end_char - start_char),
    );
    // The parsing snapshot is an owned copy. Cache against the original
    // source object's payload so repeated START conversions retain identity.
    let actual_source = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let start_byte = EmacsBytePos::new(crate::emacs_core::string_pos_cache::string_char_to_byte(
        args[0],
        actual_source,
        start_char,
    ));
    let range = super::value_reader::CharacterReadRange::for_original(&args[0], range, start_byte)
        .map_err(signal_reader_error_from_string)?;
    let read_result = read_source.read_one_character_range_with_locate_syms_with_symbols(
        range,
        symbol_position_mode,
        vars,
        shorthands.into(),
        mode,
    );
    let (value, end_position) = read_result
        .map_err(signal_reader_error_from_string)?
        .ok_or_else(|| signal(LispCondition::EndOfFile, vec![]))?;
    let absolute_end_char = end_position.get();

    Ok(Value::cons(value, Value::fixnum(absolute_end_char as i64)))
}

// ---------------------------------------------------------------------------
// 2. read
// ---------------------------------------------------------------------------

/// GNU `source_function_get/unget` (`src/lread.c:486-496`) drives the same
/// parser as strings and buffers. Callback get/unget follow GNU token boundaries;
/// the source owns callback errors and returns them before parser errors.
pub(crate) fn read_from_callable(
    ctx: &mut crate::emacs_core::eval::Context,
    func: Value,
    symbol_position_mode: SymbolPositionMode,
) -> EvalResult {
    let value = super::value_reader::read_from_callable(ctx, func, symbol_position_mode)?;
    ctx.obarray_mut().materialize_read_symbols(value);
    Ok(value)
}

// ---------------------------------------------------------------------------
// 5. read-from-minibuffer
// ---------------------------------------------------------------------------

/// Where a minibuffer read obtains its input.
///
/// GNU Emacs does not use `noninteractive` alone to select stdin:
/// `read_minibuf` enters the command loop while a keyboard macro is executing,
/// even in batch mode.  Keeping that semantic decision separate from the
/// presence of a live terminal receiver prevents individual reader builtins
/// from accidentally disagreeing about batch keyboard macros.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferInputSource {
    CommandLoop,
    StandardInput,
}

pub(crate) trait KeyboardInputRuntime {
    fn pop_unread_command_event(&mut self) -> Option<Value>;
    fn peek_unread_command_event(&self) -> Option<Value>;
    fn replace_unread_command_event_with_singleton(&mut self, event: Value);
    fn record_input_event(&mut self, event: Value);
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn record_nonmenu_input_event(&mut self, event: Value);
    fn set_read_command_keys(&mut self, keys: Vec<Value>);
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn clear_read_command_keys(&mut self);
    fn read_command_keys(&self) -> &[Value];
    fn has_input_receiver(&self) -> bool;
    fn is_executing_keyboard_macro(&self) -> bool;
    fn minibuffer_input_source(&self) -> MinibufferInputSource {
        if self.has_input_receiver() || self.is_executing_keyboard_macro() {
            MinibufferInputSource::CommandLoop
        } else {
            MinibufferInputSource::StandardInput
        }
    }
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn read_char_blocking(&mut self) -> Result<Value, Flow>;
    fn read_char_with_timeout(
        &mut self,
        timeout: Option<Duration>,
        tty_input_decoding: neovm_value::system_vocab::input::TtyInputDecoding,
    ) -> Result<Option<Value>, Flow>;
    fn read_key_sequence_blocking(
        &mut self,
        options: neovm_value::system_vocab::input::ReadKeySequenceOptions,
    ) -> Result<(Vec<Value>, Value), Flow>;
    fn symbol_value_or_nil(&self, name: &str) -> Value;
}

impl KeyboardInputRuntime for super::eval::Context {
    fn pop_unread_command_event(&mut self) -> Option<Value> {
        super::eval::Context::pop_unread_command_event(self)
    }

    fn peek_unread_command_event(&self) -> Option<Value> {
        super::eval::Context::peek_unread_command_event(self)
    }

    fn replace_unread_command_event_with_singleton(&mut self, event: Value) {
        super::eval::Context::replace_unread_command_event_with_singleton(self, event);
    }

    fn record_input_event(&mut self, event: Value) {
        super::eval::Context::record_input_event(self, event);
    }

    fn record_nonmenu_input_event(&mut self, event: Value) {
        super::eval::Context::record_nonmenu_input_event(self, event);
    }

    fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        super::eval::Context::set_read_command_keys(self, keys);
    }

    fn clear_read_command_keys(&mut self) {
        super::eval::Context::clear_read_command_keys(self);
    }

    fn read_command_keys(&self) -> &[Value] {
        super::eval::Context::read_command_keys(self)
    }

    fn has_input_receiver(&self) -> bool {
        super::eval::Context::has_input_receiver(self)
    }

    fn is_executing_keyboard_macro(&self) -> bool {
        // GNU's `read_minibuf` consults `Vexecuting_kbd_macro`, not an
        // independent command-loop flag.  The Lisp variable is special and
        // may be dynamically bound by batch callers that feed
        // `unread-command-events`; use its visible value so that real macro
        // playback and those scoped callers share one semantic authority.
        self.visible_variable_value_or_nil("executing-kbd-macro")
            .is_truthy()
    }

    fn read_char_blocking(&mut self) -> Result<Value, Flow> {
        super::eval::Context::read_char(self)
    }

    fn read_char_with_timeout(
        &mut self,
        timeout: Option<Duration>,
        tty_input_decoding: neovm_value::system_vocab::input::TtyInputDecoding,
    ) -> Result<Option<Value>, Flow> {
        super::eval::Context::read_char_with_timeout_decoding(self, timeout, tty_input_decoding)
    }

    fn read_key_sequence_blocking(
        &mut self,
        options: neovm_value::system_vocab::input::ReadKeySequenceOptions,
    ) -> Result<(Vec<Value>, Value), Flow> {
        let (keys, binding) = super::eval::Context::read_key_sequence_with_options(self, options)?;
        Ok((keys, binding.executed()))
    }

    fn symbol_value_or_nil(&self, name: &str) -> Value {
        self.variable_value_by_name(name).unwrap_or(Value::NIL)
    }
}

// ---------------------------------------------------------------------------
// 15. yes-or-no-p
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// Reader/load side effect on the moving obarray.
///
/// GNU's reader interns symbol tokens into `Vobarray` while reading;
/// Neomacs' value reader allocates canonical symbol ids first, so callers
/// that read source must apply the same obarray side effect. The helper
/// lives with the reader (its only production callers) and extends the
/// moving `Obarray`, so the symbol module names no byte-code item.
pub(crate) trait ObarrayReadSymbolsExt {
    fn materialize_read_symbols(&mut self, value: Value);
    fn materialize_read_symbols_1(&mut self, value: Value, seen: &mut rustc_hash::FxHashSet<usize>);
}

impl ObarrayReadSymbolsExt for Obarray {
    /// Materialize the symbols read from Lisp source in the active global
    /// obarray.  GNU's reader interns symbol tokens into `Vobarray` while
    /// reading; Neomacs' value reader allocates canonical symbol ids first,
    /// so callers that read source must apply the same obarray side effect.
    fn materialize_read_symbols(&mut self, value: Value) {
        // Cycle detection must use object *identity*, not `Value`'s `==`
        // (which is structural `equal`).  A `Vec` + `contains` here was
        // O(n^2) deep-`equal` over every loaded form -- the dominant cost of
        // startup.  Track visited heap objects by their tagged-pointer bits.
        let mut seen = rustc_hash::FxHashSet::default();
        self.materialize_read_symbols_1(value, &mut seen);
    }

    fn materialize_read_symbols_1(
        &mut self,
        value: Value,
        seen: &mut rustc_hash::FxHashSet<usize>,
    ) {
        match value.kind() {
            ValueKind::Symbol(id) => self.ensure_interned_global_id(id),
            ValueKind::Cons => {
                if !seen.insert(value.bits()) {
                    return;
                }
                self.materialize_read_symbols_1(value.cons_car(), seen);
                self.materialize_read_symbols_1(value.cons_cdr(), seen);
            }
            ValueKind::Veclike(
                VecLikeType::Vector
                | VecLikeType::Record
                | VecLikeType::Lambda
                | VecLikeType::Macro,
            ) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(slots) = value
                    .as_vector_data()
                    .or_else(|| value.as_record_data())
                    .or_else(|| value.closure_slots())
                {
                    for slot in slots.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(slots) = value.char_table_external_slots() {
                    for slot in slots {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::SubCharTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(table) = value.as_sub_char_table_obj() {
                    for slot in table.contents.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(table) = value.as_hash_table() {
                    for key_value in table.key_snapshots().copied() {
                        self.materialize_read_symbols_1(key_value, seen);
                    }
                    for value in table.data.values().copied() {
                        self.materialize_read_symbols_1(value, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                if !seen.insert(value.bits()) {
                    return;
                }
                if let Some(bytecode) = value.get_bytecode_data() {
                    self.materialize_read_symbols_1(bytecode.arglist, seen);
                    for constant in bytecode.constants.iter().copied() {
                        self.materialize_read_symbols_1(constant, seen);
                    }
                    if let Some(env) = bytecode.env {
                        self.materialize_read_symbols_1(env, seen);
                    }
                    if let Some(doc_form) = bytecode.doc_form {
                        self.materialize_read_symbols_1(doc_form, seen);
                    }
                    if let Some(interactive) = bytecode.interactive {
                        self.materialize_read_symbols_1(interactive, seen);
                    }
                    for slot in bytecode.extra_slots.iter().copied() {
                        self.materialize_read_symbols_1(slot, seen);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
                if let Some(symbol) = value.as_symbol_with_pos_sym() {
                    self.materialize_read_symbols_1(symbol, seen);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/minibuffer_teardown_test.rs"]
mod minibuffer_teardown_tests;
#[cfg(test)]
#[path = "tests/minibuffer_unwind_order_test.rs"]
mod minibuffer_unwind_order_tests;
#[cfg(test)]
#[path = "tests/raw_bytes_test.rs"]
mod raw_bytes_tests;
#[cfg(test)]
#[path = "tests/reader_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/shorthand_character_domain.rs"]
mod shorthand_character_domain_tests;
