//! Reader/printer builtins: read-from-string, read, prin1-to-string (enhanced),
//! format-spec, and various interactive-input stubs.

use super::error::{EvalResult, Flow, signal};
#[cfg(test)]
use super::intern::resolve_sym;
use super::intern::{SymId, intern};
use super::minibuffer::{MinibufferEntryRejection, RecursiveMinibufferPolicy};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
// storage imports removed — now using emacs_char directly
use super::symbol::Obarray;
use super::value::*;
use crate::buffer::{EmacsBytePos, EmacsByteRange, LispCharPos1};
use std::io::Write;
use std::time::Duration;
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn reader_initial_contents(
    value: Option<&Value>,
) -> Result<Option<super::minibuffer::MinibufferInitialContents>, Flow> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value.kind() {
        ValueKind::Nil => Ok(None),
        ValueKind::String => Ok(value
            .as_lisp_string()
            .cloned()
            .map(super::minibuffer::MinibufferInitialContents::at_end)),
        ValueKind::Cons => {
            let text_value = value.cons_car();
            let text = text_value.as_lisp_string().cloned().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), text_value],
                )
            })?;
            let position_value = value.cons_cdr();
            if position_value.is_nil() {
                return Ok(Some(super::minibuffer::MinibufferInitialContents::at_end(
                    text,
                )));
            }
            let position = position_value.as_fixnum().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), position_value],
                )
            })?;
            let cursor = usize::try_from(position.saturating_sub(1).max(0))
                .map(crate::buffer::CharPos0::new)
                .map_err(|_| signal(LispCondition::EndOfBuffer, vec![]))?;
            super::minibuffer::MinibufferInitialContents::at_character_offset(text, cursor)
                .map(Some)
                .ok_or_else(|| signal(LispCondition::EndOfBuffer, vec![]))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum RequireMatchSymbol {
    #[strum(to_string = "t")]
    T,
    Confirm,
    ConfirmAfterCompletion,
}

impl RequireMatchSymbol {
    fn from_lisp_value(value: Value) -> Option<Self> {
        value.as_symbol_name().and_then(|name| name.parse().ok())
    }
}

fn minibuffer_text_properties_enabled_in_buffer(
    obarray: &Obarray,
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
) -> bool {
    let option = intern("minibuffer-allow-text-properties");
    buffer_id
        .and_then(|buffer_id| buffers.get(buffer_id))
        .and_then(|buffer| buffer.get_buffer_local_by_sym_id(option))
        // A localized symbol's ordinary value cell may be cached for a
        // different buffer.  The absence of a local binding means its
        // defcell is authoritative, just as GNU's BLV lookup specifies.
        .or_else(|| obarray.default_value_id(option).copied())
        .is_some_and(|value| value.is_truthy())
}

#[derive(Clone, Copy, Debug)]
struct MinibufferHistorySpec {
    variable_value: Value,
    history_name: Option<SymId>,
    position: Value,
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

fn minibuffer_history_spec(hist_arg: Option<&Value>) -> MinibufferHistorySpec {
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

fn initialize_unbound_minibuffer_history(
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

fn minibuffer_history_limit(obarray: &Obarray, history_name: SymId) -> Option<usize> {
    let configured = obarray
        .get_property_id(history_name, intern("history-length"))
        .or_else(|| obarray.symbol_value("history-length").copied());

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
    obarray: &mut Obarray,
    history_name: SymId,
    value: &crate::heap_types::LispString,
) {
    if value.as_bytes().is_empty() {
        return;
    }

    let new_value = Value::heap_string(value.clone());
    let current = obarray.symbol_value_id_or_nil(history_name);
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

    if obarray
        .symbol_value("history-delete-duplicates")
        .is_some_and(|value| value.is_truthy())
    {
        history_items.retain(|entry| *entry != new_value);
    }

    history_items.insert(0, new_value);

    match minibuffer_history_limit(obarray, history_name) {
        Some(0) => history_items.clear(),
        Some(max) if history_items.len() > max => history_items.truncate(max),
        _ => {}
    }

    obarray.set_symbol_value_id(history_name, Value::list(history_items));
}

fn history_add_new_input_enabled(obarray: &Obarray) -> bool {
    obarray
        .symbol_value("history-add-new-input")
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

fn minibuffer_history_entry(
    result: &crate::heap_types::LispString,
    default: Value,
) -> Option<crate::heap_types::LispString> {
    if result.as_bytes().is_empty() {
        default_minibuffer_string(default)
    } else {
        Some(result.clone())
    }
}

fn add_minibuffer_history_after_unwind(
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
        add_to_minibuffer_history_variable(&mut eval.obarray, history_name, entry);
    }

    if !entry.as_bytes().is_empty() {
        let max_length =
            minibuffer_history_limit(&eval.obarray, history_name).unwrap_or(usize::MAX);
        eval.minibuffers
            .add_to_history_lisp(history_name, entry.clone(), max_length);
    }
    Ok(Value::NIL)
}

fn finish_minibuffer_result_after_unwind(
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
        read_from_string_impl(&eval.obarray, vec![Value::heap_string(input.clone())])?;
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

fn expect_lisp_string(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
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

pub(crate) fn parse_optional_read_seconds_arg(
    value: Option<&Value>,
) -> Result<Option<Duration>, Flow> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_nil() {
        return Ok(None);
    }

    let seconds = value.as_number_f64().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        )
    })?;
    if seconds <= 0.0 {
        return Ok(Some(Duration::ZERO));
    }

    Ok(Some(Duration::from_secs_f64(seconds)))
}

fn expect_initial_input_stringish(value: &Value) -> Result<(), Flow> {
    reader_initial_contents(Some(value)).map(drop)
}

fn expect_completing_read_initial_input(value: &Value) -> Result<(), Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::String => Ok(()),
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            if !pair_car.is_string() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), pair_car],
                ));
            }
            if !(pair_cdr.is_fixnum() || pair_cdr.as_char().is_some()) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("number-or-marker-p"), pair_cdr],
                ));
            }
            Ok(())
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

/// Frame that invoked the minibuffer.  Kept distinct from the frame that owns
/// its minibuffer window so those roles cannot be swapped at a call site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CallingFrame(crate::window::FrameId);

/// Frame whose window tree physically contains the active minibuffer window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MinibufferOwnerFrame(crate::window::FrameId);

#[derive(Clone, Copy, Debug)]
struct ActiveMinibufferWindowState {
    minibuffer_frame: MinibufferOwnerFrame,
    minibuffer_window_id: crate::window::WindowId,
    calling_frame: CallingFrame,
    calling_selected_window: crate::window::WindowId,
    previous_minibuffer_frame_selected_window: crate::window::WindowId,
    previous_minibuffer_buffer: Option<crate::buffer::BufferId>,
    previous_minibuffer_window_start: LispCharPos1,
    previous_minibuffer_point: LispCharPos1,
    previous_minibuffer_selected_window: Option<crate::window::WindowId>,
    previous_active_minibuffer_window: Option<crate::window::WindowId>,
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
    BufferRestored(crate::window::WindowId),
}

impl MinibufferWindowRestoreEffect {
    fn apply(self, eval: &mut super::eval::Context) {
        if let Self::BufferRestored(window) = self {
            // GNU `set_window_buffer` calls `wset_update_mode_line`; after
            // `read_minibuf_unwind` has restored the caller's selection this
            // mini-window is normally nonselected, so `wset_redisplay` also
            // raises `windows_or_buffers_changed` and rebuilds the menu bar.
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
    Replace(crate::window::WindowId),
}

impl MinibufferSelectedWindowUpdate {
    pub(crate) fn for_entry(
        level: super::minibuffer::MinibufferEntryLevel,
        selected_window: crate::window::WindowId,
        minibuffer_window: crate::window::WindowId,
    ) -> Self {
        match level {
            super::minibuffer::MinibufferEntryLevel::Outermost => Self::Replace(selected_window),
            super::minibuffer::MinibufferEntryLevel::Recursive
                if selected_window == minibuffer_window =>
            {
                Self::Preserve
            }
            super::minibuffer::MinibufferEntryLevel::Recursive => Self::Replace(selected_window),
        }
    }

    pub(crate) fn apply(self, target: &mut Option<crate::window::WindowId>) {
        if let Self::Replace(window) = self {
            *target = Some(window);
        }
    }
}

/// How the recursive edit completed, recorded before the session unwind runs.
/// `Pending` covers failures in mode/setup hooks before recursive edit starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MinibufferExitDisposition {
    Pending,
    Accepted,
    Aborted,
}

/// Semantic result of the recursive command loop.  The command loop's
/// low-level `Flow::Throw` representation must not leak into lifecycle code:
/// only `(throw 'exit nil)` is an accepted minibuffer read; every other flow
/// is an abort that must be propagated after cleanup.
enum MinibufferCommandOutcome {
    Accepted,
    Aborted(Flow),
}

impl MinibufferCommandOutcome {
    fn from_recursive_edit(result: EvalResult) -> Self {
        match result {
            Ok(_) => Self::Accepted,
            Err(Flow::Throw(ref thrown))
                if thrown.tag.is_symbol_named("exit") && !thrown.value.is_truthy() =>
            {
                Self::Accepted
            }
            Err(flow) => Self::Aborted(flow),
        }
    }

    fn disposition(&self) -> MinibufferExitDisposition {
        match self {
            Self::Accepted => MinibufferExitDisposition::Accepted,
            Self::Aborted(_) => MinibufferExitDisposition::Aborted,
        }
    }
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
    minibuffer_selected_window: &mut Option<crate::window::WindowId>,
    active_minibuffer_window: &mut Option<crate::window::WindowId>,
    minibuf_id: crate::buffer::BufferId,
    entry_level: super::minibuffer::MinibufferEntryLevel,
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
fn record_active_minibuffer_selection(
    eval: &mut super::eval::Context,
    active: ActiveMinibufferWindowState,
    minibuf_id: crate::buffer::BufferId,
) -> EvalResult {
    let _ = eval
        .frames
        .note_window_selected(active.minibuffer_window_id);
    super::window_cmds::record_buffer_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        minibuf_id,
        active.minibuffer_frame.0,
    )?;
    if !eval.buffers.buffer_hooks_inhibited(minibuf_id) {
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
        &mut eval.buffers,
        buffer_id,
        active.calling_frame.0,
    )?;
    if !eval.buffers.buffer_hooks_inhibited(buffer_id) {
        super::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn activate_minibuffer_window(
    eval: &mut super::eval::Context,
    minibuf_id: crate::buffer::BufferId,
    entry_level: super::minibuffer::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    activate_minibuffer_window_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        &mut eval.minibuffer_selected_window,
        &mut eval.active_minibuffer_window,
        minibuf_id,
        entry_level,
    )
}

fn restore_minibuffer_window_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<crate::window::WindowId>,
    active_minibuffer_window: &mut Option<crate::window::WindowId>,
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
    window_restore
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
    minibuffer_selected_window: &mut Option<crate::window::WindowId>,
    active_minibuffer_window: &mut Option<crate::window::WindowId>,
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
    let exit_hook_result = match shared.run_hook_if_bound("minibuffer-exit-hook") {
        Ok(value) => Ok(value),
        Err(Flow::Signal(_)) => Ok(Value::NIL),
        Err(flow) => Err(flow),
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
        let _ = shared.buffers.switch_current_unrecorded(state.minibuf_id);
        let shared_ptr = std::ptr::NonNull::from(&mut *shared);
        teardown_minibuffer_level_in_state(
            &mut shared.frames,
            &mut shared.buffers,
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
    } else {
        erase_expired_minibuffer_buffer_in_state(&mut shared.buffers, state.minibuf_id);
        MinibufferTeardownOutcome {
            inactive_mode_result: run_minibuffer_mode_if_bound(shared, "minibuffer-inactive-mode"),
            window_restore: MinibufferWindowRestoreEffect::NoBufferRestored,
        }
    };
    teardown_outcome.window_restore.apply(shared);
    let inactive_mode_result = teardown_outcome.inactive_mode_result;

    if let Some(buffer_id) = state.saved_buffer_id
        && shared.buffers.get(buffer_id).is_some()
    {
        shared.buffers.switch_current(buffer_id);
    }
    let selection_record_result = restored_calling_selection
        .map(|active| record_restored_calling_window_selection(shared, active))
        .unwrap_or(Ok(Value::NIL));
    shared.obarray.set_symbol_value(
        "minibuffer-depth",
        Value::fixnum(shared.minibuffers.depth() as i64),
    );
    shared
        .obarray
        .set_symbol_value("current-prefix-arg", state.saved_current_prefix_arg);
    shared.obarray.set_symbol_value(
        "minibuffer-history-variable",
        state.saved_minibuffer_history_variable,
    );
    shared.obarray.set_symbol_value(
        "minibuffer-history-position",
        state.saved_minibuffer_history_position,
    );
    shared.set_command_key_sequences(state.saved_command_keys, state.saved_raw_command_keys);

    tracing::debug!(
        "read-from-minibuffer: unwound current_buffer={:?} depth={} active_window={:?} selected_window={:?}",
        shared.buffers.current_buffer_id(),
        shared.minibuffers.depth(),
        shared.active_minibuffer_window,
        shared
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window)
    );

    exit_hook_result?;
    inactive_mode_result?;
    selection_record_result?;
    Ok(Value::NIL)
}

fn find_or_create_minibuffer_buffer_in_state(
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
fn minibuffer_ambient_directory_in_state(buffers: &crate::buffer::BufferManager) -> Option<Value> {
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

fn install_minibuffer_ambient_directory_in_state(
    buffers: &mut crate::buffer::BufferManager,
    minibuf_id: crate::buffer::BufferId,
    ambient_directory: Option<Value>,
) {
    if let Some(directory) = ambient_directory {
        let _ = buffers.set_buffer_local_property(minibuf_id, "default-directory", directory);
    }
}

fn run_minibuffer_mode_if_bound(eval: &mut super::eval::Context, mode: &str) -> EvalResult {
    if eval.obarray().symbol_function(mode).is_some() {
        eval.apply0(Value::symbol(mode))
    } else {
        Ok(Value::NIL)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn restore_minibuffer_window(eval: &mut super::eval::Context, saved: ActiveMinibufferWindowState) {
    let effect = restore_minibuffer_window_in_state(
        &mut eval.frames,
        &mut eval.buffers,
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

fn signal_invalid_read_syntax_in_buffer_object(
    buffer: &crate::buffer::Buffer,
    absolute_error_pos: usize,
    message: String,
) -> Flow {
    let accessible = buffer.accessible_emacs_byte_region();
    let end = accessible.clamp(EmacsBytePos::new(absolute_error_pos));
    let range = EmacsByteRange::new(accessible.start(), end);
    let mut prefix = Vec::with_capacity(range.len().get());
    buffer.copy_emacs_byte_range_to(range, &mut prefix);
    let line = prefix.iter().filter(|&&byte| byte == b'\n').count() as i64 + 1;
    let line_start = prefix
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let column = if buffer.get_multibyte() {
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
fn read_from_active_load_cursor(
    ctx: &mut crate::emacs_core::eval::Context,
    locate_syms: bool,
) -> EvalResult {
    let Some(cursor) = ctx.load_read_cursors.last() else {
        // `standard-input` names the load stream but no load is active: treat
        // as a spent stream (EOF) rather than crashing.
        return Err(ReadSourceObject::Anonymous.end_of_file_error());
    };
    let source = cursor.source;
    let eof_source = cursor.eof_source;
    let pos = cursor.pos;
    let shorthands = cursor.shorthands.clone();

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
        .read_one_range_with_locate_syms(pos, end, locate_syms, &ctx.obarray, shorthands.as_ref())
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

fn signal_reader_error_from_string(e: super::value_reader::ReadError) -> Flow {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => {
            ReadSourceObject::Anonymous.end_of_file_error()
        }
        super::value_reader::ReadErrorKind::Error => {
            signal("error", vec![Value::string(e.message)])
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => signal(
            LispCondition::InvalidReadSyntax,
            vec![Value::string(e.message)],
        ),
        super::value_reader::ReadErrorKind::Signal => {
            signal(e.signal_symbol.as_deref().unwrap_or("error"), e.signal_data)
        }
    }
}

fn signal_reader_error_from_buffer(
    buffer: &crate::buffer::Buffer,
    e: super::value_reader::ReadError,
) -> Flow {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => {
            ReadSourceObject::Buffer(Value::make_buffer(buffer.id)).end_of_file_error()
        }
        super::value_reader::ReadErrorKind::Error => {
            signal("error", vec![Value::string(e.message)])
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => {
            signal_invalid_read_syntax_in_buffer_object(buffer, e.position, e.message)
        }
        super::value_reader::ReadErrorKind::Signal => {
            signal(e.signal_symbol.as_deref().unwrap_or("error"), e.signal_data)
        }
    }
}

fn stdin_end_of_file_error() -> Flow {
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
/// global binding is normally nil — hence we go through
/// `visible_runtime_variable_value_by_id` rather than the raw obarray slot.
/// Returns `None` when unset/nil.
fn current_read_symbol_shorthands(
    eval: &super::eval::Context,
) -> Option<super::value_reader::ReadSymbolShorthands> {
    let sym = crate::emacs_core::intern::intern("read-symbol-shorthands");
    let value = eval
        .visible_runtime_variable_value_by_id(sym)
        .ok()
        .flatten()?;
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
    let result = read_from_string_impl_inner(&ctx.obarray, args, false, shorthands.as_ref())?;
    if result.is_cons() {
        ctx.obarray_mut()
            .materialize_read_symbols(result.cons_car());
    }
    Ok(result)
}

pub(crate) fn read_from_string_impl(
    obarray: &crate::emacs_core::symbol::Obarray,
    args: Vec<Value>,
) -> EvalResult {
    read_from_string_impl_inner(obarray, args, false, None)
}

fn read_from_string_impl_inner(
    obarray: &crate::emacs_core::symbol::Obarray,
    args: Vec<Value>,
    locate_syms: bool,
    shorthands: Option<&super::value_reader::ReadSymbolShorthands>,
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
    // byte offsets through `string_char_to_byte`, and reports
    // FINAL-STRING-INDEX as a *character* index too. Indexing by raw
    // UTF-8 byte length here was a long-standing bug (audit §11.6) that
    // would either panic on multibyte input (slicing mid-codepoint) or
    // return a byte offset where elisp expected a character count.
    let full_string_bytes = full_string.as_bytes();
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

    let start_byte = if full_string.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(full_string_bytes, start_char)
    } else {
        start_char
    };
    let end_byte = if full_string.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(full_string_bytes, end_char)
    } else {
        end_char
    };

    let read_result = read_source.read_one_range_with_locate_syms(
        start_byte,
        end_byte,
        locate_syms,
        obarray,
        shorthands,
    );

    let (value, absolute_end_byte) = read_result
        .map_err(signal_reader_error_from_string)?
        .ok_or_else(|| signal(LispCondition::EndOfFile, vec![]))?;

    let absolute_end_char = if full_string.is_multibyte() {
        crate::emacs_core::emacs_char::byte_to_char_pos(full_string_bytes, absolute_end_byte)
    } else {
        absolute_end_byte
    };

    Ok(Value::cons(value, Value::fixnum(absolute_end_char as i64)))
}

// ---------------------------------------------------------------------------
// 2. read
// ---------------------------------------------------------------------------

/// `(read &optional STREAM)`
///
/// Read one Lisp expression from STREAM.
/// - If STREAM is a string, read from that string (equivalent to car of read-from-string).
/// - If STREAM is nil, read from `standard-input`.
/// - If STREAM is a buffer, read from buffer at point.
/// - If STREAM is a marker, read from its buffer and advance only the marker.
pub fn builtin_read(ctx: &mut crate::emacs_core::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_read_impl(ctx, args, false)
}

/// Closed dispatch plan for Lisp reader streams after nil has resolved through
/// `standard-input`.  Keeping the opaque load token as its own variant makes
/// that privileged route identity-only; an ordinary symbol can never reach it
/// merely because its printed name happens to match.
#[derive(Clone, Copy, Debug)]
enum ResolvedReadStream {
    Empty,
    String(Value),
    Marker(Value),
    Buffer(crate::buffer::BufferId),
    ActiveLoadCursor,
    Minibuffer,
    FunctionSymbol(Value),
    Unsupported(Value),
}

impl ResolvedReadStream {
    fn classify(stream: Value, load_token: crate::emacs_core::eval::LoadReadStreamToken) -> Self {
        match stream.kind() {
            ValueKind::Nil => Self::Empty,
            ValueKind::T => Self::Minibuffer,
            ValueKind::String => Self::String(stream),
            ValueKind::Symbol(symbol) if load_token.identifies(symbol) => Self::ActiveLoadCursor,
            ValueKind::Symbol(_) => Self::FunctionSymbol(stream),
            ValueKind::Veclike(VecLikeType::Marker) => Self::Marker(stream),
            ValueKind::Veclike(VecLikeType::Buffer) => {
                Self::Buffer(stream.as_buffer_id().expect("buffer value kind"))
            }
            ValueKind::Fixnum(_)
            | ValueKind::Cons
            | ValueKind::Float
            | ValueKind::Subr(_)
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => Self::Unsupported(stream),
        }
    }
}

/// Shared implementation for `read` and `read-positioning-symbols`.
/// When `locate_syms` is true, every interned symbol (except nil) is
/// wrapped in a `symbol-with-pos` object carrying its source byte offset.
pub fn builtin_read_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
    locate_syms: bool,
) -> EvalResult {
    expect_max_args("read", &args, 1)?;

    let stream = if args.is_empty() || args[0].is_nil() {
        ctx.obarray
            .symbol_value("standard-input")
            .copied()
            .unwrap_or(Value::NIL)
    } else {
        args[0]
    };

    let shorthands = current_read_symbol_shorthands(ctx);
    match ResolvedReadStream::classify(stream, ctx.load_read_stream_token) {
        ResolvedReadStream::Empty => {
            // In batch/non-interactive runs, stdin-backed read signals EOF.
            Err(signal(
                LispCondition::EndOfFile,
                vec![Value::string("End of file during parsing")],
            ))
        }
        ResolvedReadStream::String(stream) => {
            // Read from string
            let result = read_from_string_impl_inner(
                &ctx.obarray,
                vec![stream],
                locate_syms,
                shorthands.as_ref(),
            )?;
            // Return just the car (the parsed object)
            match result.kind() {
                ValueKind::Cons => {
                    let pair_car = result.cons_car();
                    ctx.obarray_mut().materialize_read_symbols(pair_car);
                    Ok(pair_car)
                }
                _ => Ok(result),
            }
        }
        ResolvedReadStream::Marker(stream) => {
            let Some((Some(buf_id), Some(position), _)) =
                super::marker::marker_logical_fields(&stream)
            else {
                return Err(ReadSourceObject::Anonymous.end_of_file_error());
            };

            let (read_result, new_position) = {
                let buf = ctx
                    .buffers
                    .get(buf_id)
                    .ok_or_else(|| ReadSourceObject::Anonymous.end_of_file_error())?;
                let start = buf.char_pos_to_emacs_byte_pos_clamped(position.to_char_pos());
                let end = buf.accessible_emacs_byte_region().end();
                if start >= end {
                    return Err(ReadSourceObject::Anonymous.end_of_file_error());
                }

                match super::value_reader::read_one_from_buffer_with_locate_syms(
                    buf,
                    EmacsByteRange::new(start, end),
                    super::value_reader::BufferReadcharOffsetOrigin::Zero,
                    locate_syms,
                    &ctx.obarray,
                    shorthands.as_ref(),
                ) {
                    Ok((value, new_byte_position)) => (
                        Ok(value),
                        buf.emacs_byte_pos_to_lisp_char_pos(new_byte_position),
                    ),
                    Err(error) => {
                        // GNU's source_marker_get/unget updates the marker as
                        // each character is consumed, including on a reader
                        // error.  The value reader reports that consumed byte
                        // position, so publish it before propagating the
                        // matching Lisp signal.
                        let new_byte_position =
                            crate::buffer::EmacsBytePos::new(error.position).min(end);
                        let new_position = buf.emacs_byte_pos_to_lisp_char_pos(new_byte_position);
                        // A marker identifies its cursor but GNU does not add
                        // buffer line/column context to reader signals for it.
                        // Keep marker errors source-less, like string streams.
                        (Err(signal_reader_error_from_string(error)), new_position)
                    }
                }
            };

            super::marker::builtin_set_marker_in_buffers(
                &mut ctx.buffers,
                &[
                    stream,
                    Value::fixnum(new_position.as_i64()),
                    Value::make_buffer(buf_id),
                ],
            )?;
            let value =
                read_result?.ok_or_else(|| ReadSourceObject::Anonymous.end_of_file_error())?;
            ctx.obarray_mut().materialize_read_symbols(value);
            Ok(value)
        }
        ResolvedReadStream::Buffer(buf_id) => {
            let (maybe_value, new_pt) =
                {
                    let buf = ctx.buffers.get(buf_id).ok_or_else(|| {
                        signal("error", vec![Value::string("Buffer does not exist")])
                    })?;

                    let start = buf.point_emacs_byte_pos();
                    let end = buf.accessible_emacs_byte_region().end();
                    if start >= end {
                        return Err(ReadSourceObject::Buffer(Value::make_buffer(buf_id))
                            .end_of_file_error());
                    }

                    match super::value_reader::read_one_from_buffer_with_locate_syms(
                        buf,
                        EmacsByteRange::new(start, end),
                        super::value_reader::BufferReadcharOffsetOrigin::BufferPoint,
                        locate_syms,
                        &ctx.obarray,
                        shorthands.as_ref(),
                    ) {
                        Ok(result) => result,
                        Err(e) => return Err(signal_reader_error_from_buffer(buf, e)),
                    }
                };

            let _ = &mut ctx.buffers.goto_buffer_emacs_byte_pos(buf_id, new_pt);
            let value = maybe_value.ok_or_else(|| {
                ReadSourceObject::Buffer(Value::make_buffer(buf_id)).end_of_file_error()
            })?;
            ctx.obarray_mut().materialize_read_symbols(value);
            Ok(value)
        }
        ResolvedReadStream::ActiveLoadCursor => read_from_active_load_cursor(ctx, locate_syms),
        ResolvedReadStream::FunctionSymbol(stream) => {
            Err(signal(LispCondition::VoidFunction, vec![stream]))
        }
        ResolvedReadStream::Minibuffer => {
            // GNU `Fread` (lread.c): a `t` stream -- including the batch default
            // `standard-input` = t reached by `(read)` with no argument -- maps
            // to `(read-minibuffer "Lisp expression: ")`: read one line (from the
            // minibuffer interactively, or from stdin in `--batch`, prompt and
            // all) and parse it as a single Lisp expression. neomacs previously
            // signaled `end-of-file` outright, so a piped `echo '(+ 1 2)' |
            // neomacs --batch --eval '(print (read))'` couldn't read its input.
            let prompt = Value::string("Lisp expression: ");
            let input = builtin_read_from_minibuffer(ctx, vec![prompt])?;
            let result = read_from_string_impl_inner(
                &ctx.obarray,
                vec![input],
                locate_syms,
                shorthands.as_ref(),
            )?;
            match result.kind() {
                ValueKind::Cons => {
                    let pair_car = result.cons_car();
                    ctx.obarray_mut().materialize_read_symbols(pair_car);
                    Ok(pair_car)
                }
                _ => Ok(result),
            }
        }
        ResolvedReadStream::Unsupported(stream) => {
            // Unsupported stream source type for read-char function protocol.
            Err(signal(LispCondition::InvalidFunction, vec![stream]))
        }
    }
}

// ---------------------------------------------------------------------------
// 5. read-from-minibuffer
// ---------------------------------------------------------------------------

/// `(read-from-minibuffer PROMPT &optional INITIAL KEYMAP READ HIST DEFAULT INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer.
/// In interactive mode, sets up the minibuffer buffer, enters recursive-edit,
/// and returns the user's input when they press RET (exit-minibuffer).
/// In batch mode, reads one line from standard input and applies the same
/// READ/DEFAULT result processing as an interactive minibuffer read.
pub(crate) fn builtin_read_from_minibuffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if let Some(result) = builtin_read_from_minibuffer_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_read_from_minibuffer_in_eval(eval, &args)
}

fn read_from_stdin_noninteractive(prompt: &str) -> EvalResult {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => Err(stdin_end_of_file_error()),
        Ok(_) => {
            let input = line.trim_end_matches(['\n', '\r']);
            Ok(Value::string(input))
        }
        Err(_) => Err(stdin_end_of_file_error()),
    }
}

/// GNU's `read-minibuffer-restore-windows` policy, made exhaustive at the
/// runtime boundary shared by interpreted and byte-compiled minibuffer reads.
///
/// GNU `read_minibuf` installs the saved configuration as an unwind action, so
/// it runs after `minibuffer-exit-hook` and minibuffer teardown for normal
/// return, `C-g`, and errors alike.  Keeping this as an enum prevents a caller
/// from representing “restoration requested but no configuration captured”.
#[derive(Clone, Copy, Debug)]
enum MinibufferWindowRestoration {
    KeepChanges,
    Restore(MinibufferWindowRestorationPlan),
}

/// The one- or two-frame restore stack installed by GNU `read_minibuf`.
/// Caller is recorded first and the separate minibuffer-owner frame second,
/// so specpdl LIFO restores the owner frame before the caller configuration.
#[derive(Clone, Copy, Debug)]
struct MinibufferWindowRestorationPlan {
    caller: super::builtins::SavedWindowConfiguration,
    minibuffer_owner: Option<super::builtins::SavedWindowConfiguration>,
}

/// GNU's `read_minibuffer_restore_windows` -- the `DEFVAR_BOOL` cell
/// (`src/minibuf.c:2706`) that `read_minibuf` dereferences at
/// `src/minibuf.c:695` and `:702`.
///
/// A named predicate rather than an inline read because GNU's C reads one
/// `bool` global in two places, and because the swap-in
/// (`src/data.c:1573-1603`) means that global is the *current buffer's*
/// binding whenever a buffer has localised it (ledger 196).
pub(crate) fn minibuffer_restore_windows_requested(eval: &super::eval::Context) -> bool {
    eval.obarray
        .value_in_buffer(
            eval.buffers.current_buffer(),
            "read-minibuffer-restore-windows",
        )
        .is_some_and(|value| value.is_truthy())
}

impl MinibufferWindowRestoration {
    fn capture(eval: &mut super::eval::Context) -> Result<Self, Flow> {
        if !minibuffer_restore_windows_requested(eval) {
            return Ok(Self::KeepChanges);
        }

        let caller = super::builtins::SavedWindowConfiguration::capture(eval, Value::NIL)?;
        let selected_frame = eval.frames.selected_frame().map(|frame| frame.id);
        let minibuffer_owner = eval
            .frames
            .selected_frame()
            .and_then(|frame| frame.minibuffer_window)
            .and_then(|window| eval.frames.find_window_frame_id(window))
            .filter(|owner| Some(*owner) != selected_frame)
            .map(|owner| {
                super::builtins::SavedWindowConfiguration::capture(eval, Value::make_frame(owner.0))
            })
            .transpose()?;
        Ok(Self::Restore(MinibufferWindowRestorationPlan {
            caller,
            minibuffer_owner,
        }))
    }

    fn record(&self, eval: &mut super::eval::Context) {
        if let Self::Restore(plan) = *self {
            eval.record_native_unwind(
                super::eval::NativeUnwindAction::RestoreWindowConfiguration {
                    configuration: plan.caller,
                    options: super::builtins::WindowConfigurationRestoreOptions {
                        selected_frame: super::builtins::SelectedFrameRestoration::KeepSelected,
                        minibuffer_window:
                            super::builtins::MinibufferWindowRestoration::KeepCurrent,
                    },
                },
            );
            if let Some(configuration) = plan.minibuffer_owner {
                eval.record_native_unwind(
                    super::eval::NativeUnwindAction::RestoreWindowConfiguration {
                        configuration,
                        options: super::builtins::WindowConfigurationRestoreOptions {
                            selected_frame: super::builtins::SelectedFrameRestoration::RestoreSaved,
                            minibuffer_window:
                                super::builtins::MinibufferWindowRestoration::KeepCurrent,
                        },
                    },
                );
            }
        }
    }
}

/// Everything GNU captures at the boundary of one interactive read.
///
/// Keeping the caller identity beside the configuration stack makes the
/// required post-unwind frame selection impossible to forget or accidentally
/// apply before the configurations have been restored.
#[derive(Clone, Copy, Debug)]
struct MinibufferInvocationRestoration {
    calling_frame: Option<CallingFrame>,
    windows: MinibufferWindowRestoration,
}

impl MinibufferInvocationRestoration {
    fn capture(eval: &mut super::eval::Context) -> Result<Self, Flow> {
        Ok(Self {
            calling_frame: eval
                .frames
                .selected_frame()
                .map(|frame| CallingFrame(frame.id)),
            windows: MinibufferWindowRestoration::capture(eval)?,
        })
    }

    fn record(&self, eval: &mut super::eval::Context) {
        self.windows.record(eval);
    }

    fn select_calling_frame(&self, eval: &mut super::eval::Context) {
        // GNU `read_minibuf` explicitly reselects the invoking frame after
        // `unbind_to` has restored the owner/caller configuration stack.  The
        // restore options intentionally keep the then-current selected frame,
        // so this final step is distinct from configuration restoration.
        if let Some(calling_frame) = self.calling_frame
            && eval.frames.get(calling_frame.0).is_some()
        {
            let _ = eval.frames.select_frame(calling_frame.0);
        }
    }
}

pub(crate) fn finish_read_from_minibuffer_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_from_minibuffer_in_eval_with_setup(eval, args, |_| Ok(Value::NIL))
}

fn finish_read_from_minibuffer_in_eval_with_setup(
    eval: &mut super::eval::Context,
    args: &[Value],
    run_before_setup_hook: impl FnMut(&mut super::eval::Context) -> EvalResult,
) -> EvalResult {
    // There is one production lifecycle implementation for interpreted and
    // byte-compiled callers.  Keeping the evaluator path as a thin adapter
    // prevents the two paths from acquiring different early-return holes.
    finish_read_from_minibuffer_in_vm_runtime_interactive(eval, args, run_before_setup_hook)
}

pub(crate) fn builtin_read_from_minibuffer_in_runtime(
    runtime: &mut super::eval::Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-from-minibuffer", args, 1)?;
    expect_max_args("read-from-minibuffer", args, 7)?;
    let prompt = expect_lisp_string(&args[0])?;
    if let Some(initial) = args.get(1) {
        expect_initial_input_stringish(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            let result = read_from_stdin_noninteractive(
                &crate::emacs_core::emacs_char::to_utf8_lossy(prompt.as_bytes()),
            )?;
            let result = expect_lisp_string(&result)?;
            let read = args.get(3).copied().unwrap_or(Value::NIL);
            let default = args.get(5).copied().unwrap_or(Value::NIL);
            finish_minibuffer_result_after_unwind(runtime, result, read, default).map(Some)
        }
    }
}

// ---------------------------------------------------------------------------
// 6. read-string
// ---------------------------------------------------------------------------

/// `(read-string PROMPT &optional INITIAL HISTORY DEFAULT INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer.  Delegates to `read-from-minibuffer`.
pub(crate) fn builtin_read_string(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = builtin_read_string_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_read_string_in_eval(eval, &args)
}

pub(crate) fn finish_read_string_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_string_with_minibuffer(args, |minibuffer_args| {
        finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    })
}

pub(crate) fn builtin_read_string_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-string", args, 1)?;
    expect_max_args("read-string", args, 5)?;
    let prompt = args[0];
    if let Some(initial) = args.get(1) {
        expect_initial_input_stringish(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            let prompt_str = expect_lisp_string(&prompt)?;
            read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                prompt_str.as_bytes(),
            ))
            .map(Some)
        }
    }
}

pub(crate) fn finish_read_string_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let prompt = args[0];

    // (read-from-minibuffer PROMPT INITIAL nil nil HIST DEFAULT INHERIT-INPUT-METHOD)
    let initial = args.get(1).copied().unwrap_or(Value::NIL);
    let history = args.get(2).copied().unwrap_or(Value::NIL);
    let default = args.get(3).copied().unwrap_or(Value::NIL);
    let inherit = args.get(4).copied().unwrap_or(Value::NIL);

    let minibuffer_args = [
        prompt,
        initial,
        Value::NIL,
        Value::NIL,
        history,
        default,
        inherit,
    ];
    read_from_minibuffer(&minibuffer_args)
}

// ---------------------------------------------------------------------------
// 7. read-number -- NOT here
// ---------------------------------------------------------------------------
//
// `read-number' is `(defun read-number (prompt &optional default hist) ...)'
// at lisp/subr.el:3725, over `read-from-minibuffer'.  GNU has no C version,
// and the one place C reaches it -- the `n' and `N' interactive code letters
// -- goes through the FUNCTION CELL: `calln (Qread_number, callint_message)',
// src/callint.c:645.  `interactive.rs' does the same
// (`read_number_through_the_function_cell'), so nothing in Rust needs a
// `read-number' of its own (DIVERGENCES.md 152).

// ---------------------------------------------------------------------------
// 8. completing-read
// ---------------------------------------------------------------------------

const COMPLETING_READ_MAX_ARGS: usize = 8;

/// `(completing-read PROMPT COLLECTION &optional PREDICATE REQUIRE-MATCH
///                    INITIAL-INPUT HIST DEF INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer with completion.
/// In interactive mode, delegates to read-from-minibuffer with
/// minibuffer-local-completion-map (or minibuffer-local-must-match-map
/// if REQUIRE-MATCH is non-nil).
pub(crate) fn builtin_completing_read(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    validate_completing_read_arity(&args)?;
    if let Some(function) = completing_read_function_value(eval) {
        return eval.apply(function, completing_read_function_args(args));
    }

    if let Some(result) = builtin_completing_read_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_completing_read_in_eval(eval, &args)
}

pub(crate) fn finish_completing_read_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    let minibuffer_args = completing_read_minibuffer_args(eval.obarray(), args);
    let collection = args[1];
    let predicate = args.get(2).copied().unwrap_or(Value::NIL);
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let original_buffer = eval
        .buffers
        .current_buffer_id()
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL);
    let completion_ignore_case = eval
        .eval_symbol("completion-ignore-case")
        .unwrap_or(Value::NIL);

    finish_read_from_minibuffer_in_eval_with_setup(eval, &minibuffer_args, move |eval| {
        install_completing_read_minibuffer_locals(
            eval,
            collection,
            predicate,
            require_match,
            original_buffer,
            completion_ignore_case,
        );
        Ok(Value::NIL)
    })
}

pub(crate) fn builtin_completing_read_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    validate_completing_read_arity(args)?;
    let prompt = expect_lisp_string(&args[0])?;
    if let Some(initial) = args.get(4) {
        expect_completing_read_initial_input(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            // Batch/noninteractive: GNU's `Fcompleting_read` routes through
            // `read_minibuf` -> `read_minibuf_noninteractive` (minibuf.c), which
            // writes the prompt to stdout and reads the answer from stdin, exactly
            // like `read-from-minibuffer`.  Mirror that so the prompt is emitted
            // before the (likely) end-of-file signal on empty stdin.
            read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                prompt.as_bytes(),
            ))
            .map(Some)
        }
    }
}

pub(crate) fn validate_completing_read_arity(args: &[Value]) -> Result<(), Flow> {
    expect_min_args("completing-read", args, 2)?;
    expect_max_args("completing-read", args, COMPLETING_READ_MAX_ARGS)?;
    Ok(())
}

fn completing_read_function_args(mut args: Vec<Value>) -> Vec<Value> {
    // GNU's fixed-arity Fcompleting_read receives nil for omitted optional
    // arguments, then calls completing-read-function with all eight values.
    // Keep that adapter interface stable even though Neomacs builtins receive
    // only the arguments supplied by the Lisp caller.
    args.resize(COMPLETING_READ_MAX_ARGS, Value::NIL);
    args
}

pub(crate) fn completing_read_function_value(eval: &super::eval::Context) -> Option<Value> {
    eval.eval_symbol("completing-read-function")
        .ok()
        .filter(|function| !function.is_nil())
}

pub(crate) fn finish_read_from_minibuffer_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_from_minibuffer_in_vm_runtime_with_setup(shared, args, |_| Ok(Value::NIL))
}

fn finish_read_from_minibuffer_in_vm_runtime_with_setup(
    shared: &mut super::eval::Context,
    args: &[Value],
    run_before_setup_hook: impl FnMut(&mut super::eval::Context) -> EvalResult,
) -> EvalResult {
    if let Some(result) = builtin_read_from_minibuffer_in_runtime(shared, args)? {
        return Ok(result);
    }

    finish_read_from_minibuffer_in_vm_runtime_interactive(shared, args, run_before_setup_hook)
}

fn finish_read_from_minibuffer_in_vm_runtime_interactive(
    shared: &mut super::eval::Context,
    args: &[Value],
    mut run_before_setup_hook: impl FnMut(&mut super::eval::Context) -> EvalResult,
) -> EvalResult {
    // Check inhibit-interaction — GNU Emacs signals an error when any
    // interactive read is attempted while this variable is non-nil.
    if shared
        .obarray
        .symbol_value("inhibit-interaction")
        .is_some_and(|v| v.is_truthy())
    {
        return Err(signal(
            "inhibited-interaction",
            vec![Value::string(
                "Attempt to interact with user while inhibit-interaction is non-nil",
            )],
        ));
    }

    let prompt = expect_lisp_string(&args[0])?;
    let prompt_display = crate::emacs_core::emacs_char::to_utf8_lossy(prompt.as_bytes());
    let initial_input = reader_initial_contents(args.get(1))?;
    let keymap_arg = args.get(2).copied().unwrap_or(Value::NIL);
    let read_arg = args.get(3).copied().unwrap_or(Value::NIL);
    let history_spec = minibuffer_history_spec(args.get(4));
    let default_val = args.get(5).copied().unwrap_or(Value::NIL);

    let result = shared.with_unwind_scope(|shared| {
        // Root every Lisp argument through both lifecycle unwind and the
        // post-unwind history/parser phase.  Native argument slices are not GC
        // roots by themselves.
        for root in args.iter().copied() {
            shared.push_specpdl_root(root);
        }

        // GNU `read_minibuf` binds `minibuffer-default` to DEFAULT for the whole
        // read (src/minibuf.c:591) and unwinds it afterwards.  Everything that
        // offers the default to the user reads the variable, not the argument:
        // `next-history-element`/`M-n`, `minibuffer-default-add-function`, and
        // packages that observe the live minibuffer from
        // `minibuffer-setup-hook`.  It is bound here, above the recursion and
        // entry checks, so no path through this read can skip it.
        shared.try_specbind(intern("minibuffer-default"), default_val)?;

        let restoration = MinibufferInvocationRestoration::capture(shared)?;
        let lifecycle_result = shared.with_unwind_scope(|shared| {
            // Window configurations are below the session action on the inner
            // specpdl stack, so teardown runs first and configurations follow.
            restoration.record(shared);

            let recursive_policy = if shared
                .obarray
                .symbol_value("enable-recursive-minibuffers")
                .is_some_and(|value| value.is_truthy())
            {
                RecursiveMinibufferPolicy::Allow
            } else {
                RecursiveMinibufferPolicy::Reject
            };
        let depth_before_entry = shared.minibuffers.depth();
        let entry_permit = shared
            .minibuffers
            .prepare_entry(recursive_policy)
            .map_err(MinibufferEntryRejection::into_flow)?;

    // Save state.  GNU read_minibuf saves Vcurrent_prefix_arg in
    // minibuf_save_list and restores it during read_minibuf_unwind;
    // minibuffer commands may clobber it while reading input.
    let saved_buffer_id = shared.buffers.current_buffer().map(|b| b.id);
    let caller_allows_text_properties = minibuffer_text_properties_enabled_in_buffer(
        &shared.obarray,
        &shared.buffers,
        saved_buffer_id,
    );
    let saved_current_prefix_arg = shared
        .obarray
        .symbol_value("current-prefix-arg")
        .copied()
        .unwrap_or(Value::NIL);
    // GNU `read_minibuf` also saves `(this-command-keys-vector)` (minibuf.c:
    // 738-739) and `read_minibuf_unwind` restores it (minibuf.c:1144-1146) so
    // the invoking command's `this-command-keys` survives the minibuffer's own
    // command-loop reads. Byte-compiled callers (`query-replace-read-to`,
    // `register-read-with-preview`, …) reach the minibuffer through THIS VM
    // runtime path, so the save/restore must live here too — otherwise their
    // following `read-key` sees the minibuffer's terminating RET and fires its
    // idle-timer probe early.
    let saved_command_keys = shared.read_command_keys().to_vec();
    let saved_raw_command_keys = shared.read_raw_command_keys().to_vec();
    let saved_minibuffer_history_variable = shared
        .obarray
        .symbol_value("minibuffer-history-variable")
        .copied()
        .unwrap_or(Value::from_sym_id(intern("minibuffer-history")));
    let saved_minibuffer_history_position = shared
        .obarray
        .symbol_value("minibuffer-history-position")
        .copied()
        .unwrap_or(Value::NIL);
    let recursive_depth = shared.recursive_command_loop_depth();

    // GNU read_minibuf initializes an unbound requested history in the
    // caller's runtime environment before switching to the minibuffer and
    // before any mode/setup hook can inspect it (minibuf.c:765-772).
    initialize_unbound_minibuffer_history(shared, history_spec)?;

    // GNU `read_minibuf` captures the caller's directory before switching to
    // *Minibuf-N*, then installs it after minibuffer-mode has reset locals.
    let ambient_directory = minibuffer_ambient_directory_in_state(&shared.buffers);

    let minibuf_depth = entry_permit.depth();
    let minibuf_id = find_or_create_minibuffer_buffer_in_state(&mut shared.buffers, minibuf_depth);

    let active_window_state = activate_minibuffer_window_in_state(
        &mut shared.frames,
        &mut shared.buffers,
        &mut shared.minibuffer_selected_window,
        &mut shared.active_minibuffer_window,
        minibuf_id,
        entry_permit.level(),
    );
    if active_window_state.is_none() {
        shared.buffers.switch_current(minibuf_id);
    }
    let session_unwind = MinibufferSessionUnwind {
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
    };
    let session_token = shared.record_native_unwind(
        super::eval::NativeUnwindAction::MinibufferSession {
            state: Box::new(session_unwind),
        },
    );
    if let Some(active_window_state) = active_window_state {
        record_active_minibuffer_selection(shared, active_window_state, minibuf_id)?;
    }
    shared
        .obarray
        .set_symbol_value("minibuffer-history-variable", history_spec.variable_value);
    shared
        .obarray
        .set_symbol_value("minibuffer-history-position", history_spec.position);
    run_minibuffer_mode_if_bound(shared, "minibuffer-mode")?;
    install_minibuffer_ambient_directory_in_state(
        &mut shared.buffers,
        minibuf_id,
        ambient_directory,
    );

    let prompt_properties = shared
        .obarray
        .symbol_value("minibuffer-prompt-properties")
        .copied()
        .unwrap_or(Value::NIL);
    super::minibuffer::install_minibuffer_buffer_contents(
        &mut shared.buffers,
        minibuf_id,
        &prompt,
        initial_input.as_ref(),
        prompt_properties,
    );
    tracing::debug!(
        "read-from-minibuffer: prompt={:?} minibuf_id={:?} current_buffer={:?} active_window={:?} selected_window={:?}",
        prompt_display,
        minibuf_id,
        shared.buffers.current_buffer_id(),
        shared.active_minibuffer_window,
        shared
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window)
    );

    {
        let state = shared.minibuffers.enter_with_permit(
            entry_permit,
            minibuf_id,
            &prompt,
            initial_input.as_ref(),
            history_spec.history_name,
        );
        state.command_loop_depth = recursive_depth;
    }

    // GNU `read_minibuf' clears the echo area HERE -- `clear_message (1, 1)'
    // at src/minibuf.c:894, after the prompt and any initial input are in the
    // buffer and immediately before `bset_keymap (current_buffer, map)'
    // (src/minibuf.c:895) and `run_hook (Qminibuffer_setup_hook)'
    // (src/minibuf.c:900). Entry, not exit: by the time any Lisp in the session
    // runs, `current-message' is already nil, which is what a
    // `minibuffer-setup-hook' function reading it observes.
    //
    // Only on this interactive path. GNU's batch arm returns from
    // `read_minibuf_noninteractive' at src/minibuf.c:649-655, long before the
    // clear.
    let _ = shared.clear_echo_area_message_without_clear_hook();

    let minibuf_keymap = if !keymap_arg.is_nil() {
        keymap_arg
    } else {
        shared
            .obarray
            .symbol_value("minibuffer-local-map")
            .copied()
            .unwrap_or(Value::NIL)
    };
    let _ = shared.buffers.set_current_local_map(minibuf_keymap);
    shared
        .obarray
        .set_symbol_value("minibuffer-prompt", Value::heap_string(prompt.clone()));
    shared
        .obarray
        .set_symbol_value("minibuffer-depth", Value::fixnum(minibuf_depth as i64));
    run_before_setup_hook(shared)?;
    shared.run_hook_if_bound("minibuffer-setup-hook")?;

    let command_outcome =
        MinibufferCommandOutcome::from_recursive_edit(shared.minibuffer_command_loop_inner());
    match shared.native_unwind_action_mut(session_token) {
        Some(super::eval::NativeUnwindAction::MinibufferSession { state }) => {
            state.disposition = command_outcome.disposition();
        }
        other => {
            debug_assert!(false, "minibuffer unwind action disappeared: {other:?}");
        }
    }

    match command_outcome {
        MinibufferCommandOutcome::Accepted => {
            let _ = shared.buffers.switch_current_unrecorded(minibuf_id);
            let preserve_text_properties = caller_allows_text_properties
                || minibuffer_text_properties_enabled_in_buffer(
                    &shared.obarray,
                    &shared.buffers,
                    Some(minibuf_id),
                );
            Ok(Value::heap_string(
                super::minibuffer::minibuffer_contents_lisp_string_in_state(
                    &shared.obarray,
                    &shared.buffers,
                    &shared.minibuffers,
                    preserve_text_properties,
                )?,
            ))
        }
        MinibufferCommandOutcome::Aborted(flow) => Err(flow),
    }
        });

        // This is deliberately between the inner lifecycle scope and history:
        // GNU restores both configurations, then reselects the caller, then
        // calls `add-to-history` in the restored buffer-local environment.
        restoration.select_calling_frame(shared);
        // `with_unwind_scope` roots the tagged result while exit hooks and
        // window restoration allocate, so string properties cannot retain
        // otherwise-unreachable Lisp objects through an untraced Rust value.
        let result_value = lifecycle_result?;
        let result_text = result_value
            .as_lisp_string()
            .expect("an accepted minibuffer command must return its contents")
            .clone();
        if history_add_new_input_enabled(&shared.obarray)
            && let Some(history_name) = history_spec.history_name
            && let Some(history_entry) = minibuffer_history_entry(&result_text, default_val)
        {
            add_minibuffer_history_after_unwind(shared, history_name, &history_entry)?;
        }

        finish_minibuffer_result_after_unwind(shared, result_text, read_arg, default_val)
    });
    if result.is_ok() {
        shared.note_interactive_minibuffer_read();
    }
    result
}

pub(crate) fn finish_completing_read_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    if let Some(result) = builtin_completing_read_in_runtime(shared, args)? {
        return Ok(result);
    }
    let minibuffer_args = completing_read_minibuffer_args(&shared.obarray, args);
    let collection = args[1];
    let predicate = args.get(2).copied().unwrap_or(Value::NIL);
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let original_buffer = shared
        .buffers
        .current_buffer_id()
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL);
    let completion_ignore_case = shared
        .eval_symbol("completion-ignore-case")
        .unwrap_or(Value::NIL);

    finish_read_from_minibuffer_in_vm_runtime_with_setup(shared, &minibuffer_args, move |shared| {
        install_completing_read_minibuffer_locals(
            shared,
            collection,
            predicate,
            require_match,
            original_buffer,
            completion_ignore_case,
        );
        Ok(Value::NIL)
    })
}

/// Map the `REQUIRE-MATCH` argument of `completing-read` to the value
/// stored in `minibuffer-completion-confirm`.
///
/// GNU semantics:
///   nil        → nil
///   t          → nil
///   confirm    → confirm
///   confirm-after-completion → confirm-after-completion
///   function / other non-t, non-nil → unchanged
fn completion_confirm_from_require_match(require_match: Value) -> Value {
    match RequireMatchSymbol::from_lisp_value(require_match) {
        Some(RequireMatchSymbol::T) => Value::NIL,
        Some(RequireMatchSymbol::Confirm | RequireMatchSymbol::ConfirmAfterCompletion) => {
            require_match
        }
        None if require_match.is_nil() => Value::NIL,
        None => require_match,
    }
}

fn install_completing_read_minibuffer_locals(
    eval: &mut super::eval::Context,
    collection: Value,
    predicate: Value,
    require_match: Value,
    original_buffer: Value,
    completion_ignore_case: Value,
) {
    let Some(current_id) = eval.buffers.current_buffer_id() else {
        return;
    };
    for (name, value) in [
        ("minibuffer-completion-table", collection),
        ("minibuffer-completion-predicate", predicate),
        (
            "minibuffer-completion-confirm",
            completion_confirm_from_require_match(require_match),
        ),
        ("minibuffer--require-match", require_match),
        ("minibuffer--original-buffer", original_buffer),
        ("completion-ignore-case", completion_ignore_case),
    ] {
        let _ = eval.set_buffer_local_binding_by_id(current_id, intern(name), value);
    }
}

pub(crate) fn completing_read_minibuffer_args(obarray: &Obarray, args: &[Value]) -> [Value; 7] {
    let prompt = args[0];
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let initial_input = args.get(4).copied().unwrap_or(Value::NIL);
    let hist = args.get(5).copied().unwrap_or(Value::NIL);
    let default_val = args.get(6).copied().unwrap_or(Value::NIL);
    let inherit = args.get(7).copied().unwrap_or(Value::NIL);

    let keymap = if !require_match.is_nil() {
        obarray
            .symbol_value("minibuffer-local-must-match-map")
            .copied()
            .unwrap_or(Value::NIL)
    } else {
        obarray
            .symbol_value("minibuffer-local-completion-map")
            .copied()
            .unwrap_or(Value::NIL)
    };

    [
        prompt,
        initial_input,
        keymap,
        Value::NIL,
        hist,
        default_val,
        inherit,
    ]
}

fn event_to_int(event: &Value) -> Option<i64> {
    match event.kind() {
        ValueKind::Fixnum(n) => Some(n),
        _ => None,
    }
}

fn event_to_char(event: &Value) -> Option<char> {
    match event.kind() {
        ValueKind::Fixnum(c) => char::from_u32(c as u32),
        _ => None,
    }
}

fn expect_optional_prompt_string(args: &[Value]) -> Result<(), Flow> {
    if args.is_empty() || args[0].is_nil() || args[0].is_string() {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), args[0]],
    ))
}

fn non_character_input_event_error() -> Flow {
    signal("error", vec![Value::string("Non-character input-event")])
}

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

/// The deadline policy for one command-event read.
///
/// GNU's `read_filtered_event` always enters `read_char`, with or without a
/// terminal.  The low-level unified wait is the sole authority on whether
/// input can still arrive because it sees every source: keyboard input,
/// timers, processes, and file notifications.  This type carries only the
/// caller's deadline policy so upper layers cannot prematurely classify that
/// changing set of sources and bypass part of the wait loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandEventReadPlan {
    Unbounded,
    Timed { timeout: Duration },
}

impl CommandEventReadPlan {
    pub(crate) fn from_seconds(seconds: Option<&Value>) -> Result<Self, Flow> {
        Ok(match parse_optional_read_seconds_arg(seconds)? {
            Some(timeout) => Self::Timed { timeout },
            None => Self::Unbounded,
        })
    }

    pub(crate) fn timeout(self) -> Option<Duration> {
        match self {
            Self::Unbounded => None,
            Self::Timed { timeout } => Some(timeout),
        }
    }
}

/// GNU passes `prev_event == t` to its low-level reader when
/// INHERIT-INPUT-METHOD is nil.  Besides suppressing input methods, that asks a
/// TTY reader to return transport bytes without applying
/// `keyboard-coding-system`.
pub(crate) fn tty_input_decoding_from_read_args(
    args: &[Value],
) -> crate::keyboard::TtyInputDecoding {
    if args.get(1).is_some_and(|value| value.is_truthy()) {
        crate::keyboard::TtyInputDecoding::KeyboardCodingSystem
    } else {
        crate::keyboard::TtyInputDecoding::RawBytes
    }
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
        tty_input_decoding: crate::keyboard::TtyInputDecoding,
    ) -> Result<Option<Value>, Flow>;
    fn read_key_sequence_blocking(
        &mut self,
        options: crate::keyboard::ReadKeySequenceOptions,
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
        tty_input_decoding: crate::keyboard::TtyInputDecoding,
    ) -> Result<Option<Value>, Flow> {
        super::eval::Context::read_char_with_timeout_decoding(self, timeout, tty_input_decoding)
    }

    fn read_key_sequence_blocking(
        &mut self,
        options: crate::keyboard::ReadKeySequenceOptions,
    ) -> Result<(Vec<Value>, Value), Flow> {
        super::eval::Context::read_key_sequence_with_options(self, options)
    }

    fn symbol_value_or_nil(&self, name: &str) -> Value {
        self.obarray
            .symbol_value(name)
            .copied()
            .unwrap_or(Value::NIL)
    }
}

pub(crate) fn read_key_sequence_options_from_args(
    args: &[Value],
) -> crate::keyboard::ReadKeySequenceOptions {
    // GNU `Fread_key_sequence`/`Fread_key_sequence_vector` signature
    // (keyboard.c:11935) is
    //   (PROMPT CONTINUE-ECHO DONT-DOWNCASE-LAST CAN-RETURN-SWITCH-FRAME ...).
    // Arg 1 (CONTINUE-ECHO) governs whether the previous command's
    // `this-command-keys` is preserved (non-nil) or cleared for a fresh
    // sequence (nil); `read-key` (subr.el) passes nil here and relies on the
    // clear so its idle-timer `(this-command-keys-vector)` probe is empty.
    crate::keyboard::ReadKeySequenceOptions::new(
        args.first().copied().unwrap_or(Value::NIL),
        args.get(1).is_some_and(|v| v.is_truthy()),
        args.get(2).is_some_and(|v| v.is_truthy()),
        args.get(3).is_some_and(|v| v.is_truthy()),
    )
}

fn read_key_sequence_string_result(keys: &[Value]) -> Value {
    let mut chars_only = true;
    let mut s = String::new();
    for key in keys {
        if let Some(c) = event_to_char(key) {
            s.push(c);
        } else {
            chars_only = false;
            break;
        }
    }
    if chars_only {
        Value::string(s)
    } else {
        read_key_sequence_vector_result(keys)
    }
}

fn read_key_sequence_vector_result(keys: &[Value]) -> Value {
    Value::vector(
        keys.iter()
            .map(|key| event_to_int(key).map(Value::fixnum).unwrap_or(*key))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// 10. input-pending-p
// ---------------------------------------------------------------------------

/// `(input-pending-p &optional CHECK-TIMERS)`
///
/// Return non-nil when unread input, staged host input, or `quit-flag` is pending.
/// `CHECK-TIMERS` is accepted and fires due timers before checking.
fn input_pending_now(ctx: &crate::emacs_core::eval::Context) -> bool {
    ctx.has_pending_requeued_events()
        || ctx.has_pending_command_input_for_query()
        || !ctx.quit_flag_value().is_nil()
}

pub(crate) fn builtin_input_pending_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("input-pending-p", &args, 1)?;
    ctx.sync_keyboard_terminal_owner();
    ctx.service_input_pending_without_timers()?;

    if input_pending_now(ctx) {
        return Ok(Value::T);
    }

    if args.first().is_some_and(|v| v.is_truthy()) {
        // GNU `input-pending-p' can run due timers here, but it does not
        // force a redisplay the way `detect_input_pending_run_timers' does.
        ctx.service_input_pending_with_timers()?;
    }

    Ok(Value::bool_val(input_pending_now(ctx)))
}

// ---------------------------------------------------------------------------
// 11. discard-input
// ---------------------------------------------------------------------------

/// `(discard-input)`
///
/// Discard pending unread command events for the current scope.
pub(crate) fn builtin_discard_input(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("discard-input", &args, 0)?;
    super::eval::set_runtime_binding(
        &mut ctx.obarray,
        &mut ctx.buffers,
        &ctx.custom,
        ctx.specpdl.as_slice(),
        intern("unread-command-events"),
        Value::NIL,
    )?;
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 11b. insert-special-event
// ---------------------------------------------------------------------------

/// `(insert-special-event EVENT)` -> nil
///
/// Insert EVENT into the low-level special-event queue, so that the next
/// key-reading operation handles it through `special-event-map` instead of
/// returning it as ordinary user input.
///
/// Mirrors GNU `Finsert_special_event` at
/// `src/keyboard.c:12060`:
///
///   DEFUN ("insert-special-event", Finsert_special_event, ...)
///     (Lisp_Object event)
///   {
///     CHECK_CONS (event);
///     if (NILP (access_keymap (... Vspecial_event_map ..., event, ...)))
///       signal_error ("Invalid event kind", XCAR (event));
///     kbd_buffer_store_event (&ie);
///     return Qnil;
///   }
///
/// GNU pushes into the kernel kbd_buffer (which is a ring of
/// `struct input_event` records) so the event is delivered via the
/// same special-event path as hardware input. Neomacs keeps this queue in the
/// keyboard runtime (`unread_events`), not in `unread-command-events`: callers
/// like file notification rely on `read-event` consuming the event internally
/// and running the `special-event-map` handler.
///
/// Keyboard audit Finding 16 in
/// `drafts/keyboard-command-loop-audit.md`.
pub(crate) fn builtin_insert_special_event(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("insert-special-event", &args, 1)?;
    let event = args[0];
    if !event.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), event],
        ));
    }
    if ctx.special_event_binding(&event).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Invalid event kind"), event.cons_car()],
        ));
    }
    ctx.queue_special_event(event);
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 12. current-input-mode / set-input-mode
// ---------------------------------------------------------------------------

/// `(current-input-mode)` -> `(INTERRUPT FLOW META QUIT)`
pub(crate) fn builtin_current_input_mode(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-input-mode", &args, 0)?;
    let (interrupt, flow, meta, quit) = ctx.current_input_mode_tuple();
    Ok(Value::list(vec![
        Value::bool_val(interrupt),
        Value::bool_val(flow),
        Value::bool_val(meta),
        Value::fixnum(quit),
    ]))
}

/// `(set-input-mode INTERRUPT FLOW META QUIT)`
///
/// Batch-compatible behavior currently tracks INTERRUPT plus Lisp-visible
/// QUIT while leaving FLOW/META fixed.
pub(crate) fn builtin_set_input_mode(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-input-mode", &args, 3)?;
    expect_max_args("set-input-mode", &args, 4)?;
    eval.set_input_mode_interrupt(args[0].is_truthy());
    if let Some(quit) = args.get(3).copied()
        && !quit.is_nil()
    {
        set_quit_char_in_context(eval, quit)?;
    }
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 13. input mode helper setters
// ---------------------------------------------------------------------------

/// `(set-input-interrupt-mode INTERRUPT)`
pub(crate) fn builtin_set_input_interrupt_mode(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-input-interrupt-mode", &args, 1)?;
    eval.set_input_mode_interrupt(args[0].is_truthy());
    Ok(Value::NIL)
}

pub(crate) fn builtin_read_char_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("read-char"), Value::fixnum(args.len() as i64)],
        ));
    }
    expect_optional_prompt_string(args)?;
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());

    if let Some(event) = runtime.peek_unread_command_event() {
        if let Some(n) = event_to_int(&event) {
            let event = runtime
                .pop_unread_command_event()
                .expect("peeked unread event should still be present");
            if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
                runtime.set_read_command_keys(vec![event]);
            }
            return Ok(Some(Value::fixnum(n)));
        }
        runtime.replace_unread_command_event_with_singleton(event);
        runtime.record_input_event(event);
        return Err(non_character_input_event_error());
    }

    Ok(None)
}

pub(crate) fn builtin_read_key_sequence_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-key-sequence", args, 1)?;
    expect_max_args("read-key-sequence", args, 6)?;
    expect_optional_prompt_string(args)?;

    if runtime.peek_unread_command_event().is_some() {
        let (keys, _binding) =
            runtime.read_key_sequence_blocking(read_key_sequence_options_from_args(args))?;
        return Ok(Some(read_key_sequence_string_result(&keys)));
    }

    Ok(None)
}

pub(crate) fn builtin_read_key_sequence_vector_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-key-sequence-vector", args, 1)?;
    expect_max_args("read-key-sequence-vector", args, 6)?;
    expect_optional_prompt_string(args)?;

    if runtime.peek_unread_command_event().is_some() {
        let (keys, _binding) =
            runtime.read_key_sequence_blocking(read_key_sequence_options_from_args(args))?;
        return Ok(Some(read_key_sequence_vector_result(&keys)));
    }

    Ok(None)
}

/// `(set-input-meta-mode META)`
///
/// Batch-compatible behavior: accepts GNU-compatible optional TERMINAL and returns nil.
pub(crate) fn builtin_set_input_meta_mode(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-input-meta-mode", &args, 1)?;
    expect_max_args("set-input-meta-mode", &args, 2)?;
    Ok(Value::NIL)
}

/// `(set-output-flow-control FLOW)`
///
/// Batch-compatible behavior: accepts one argument and returns nil.
pub(crate) fn builtin_set_output_flow_control(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-output-flow-control", &args, 1)?;
    expect_max_args("set-output-flow-control", &args, 2)?;
    Ok(Value::NIL)
}

/// `(set-quit-char CHAR)`
///
fn set_quit_char_in_context(eval: &mut super::eval::Context, quit: Value) -> EvalResult {
    let Some(quit) = quit.as_fixnum() else {
        return Err(signal(
            "error",
            vec![Value::string("QUIT must be an ASCII character")],
        ));
    };
    if !(0..=0o400).contains(&quit) {
        return Err(signal(
            "error",
            vec![Value::string("QUIT must be an ASCII character")],
        ));
    }

    eval.set_quit_char(quit);
    Ok(Value::NIL)
}

/// GNU-compatible quit-char setter for the current evaluator.
pub(crate) fn builtin_set_quit_char(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-quit-char", &args, 1)?;
    set_quit_char_in_context(eval, args[0])
}

// ---------------------------------------------------------------------------
// 14. waiting-for-user-input-p
// ---------------------------------------------------------------------------

/// `(waiting-for-user-input-p)`
///
/// Batch-mode compatibility: always returns nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_waiting_for_user_input_p(args: Vec<Value>) -> EvalResult {
    expect_args("waiting-for-user-input-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_waiting_for_user_input_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("waiting-for-user-input-p", &args, 0)?;
    Ok(Value::bool_val(eval.waiting_for_user_input()))
}

// ---------------------------------------------------------------------------
// 15. yes-or-no-p
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// `(yes-or-no-p PROMPT)`
///
/// Ask user a yes-or-no question requiring "yes" or "no" typed in full.
/// In interactive mode, uses read-from-minibuffer.
/// In batch mode, signals end-of-file.
pub(crate) fn builtin_yes_or_no_p(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    validate_yes_or_no_p_args(&args)?;
    if let Some(result) = yes_or_no_p_dialog_result(eval, &args)? {
        return Ok(result);
    }
    if yes_or_no_p_use_short_answers(eval) {
        return eval.apply(Value::symbol("y-or-n-p"), args);
    }
    if let Some(result) = builtin_yes_or_no_p_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_yes_or_no_p_in_eval(eval, &args)
}

pub(crate) fn finish_yes_or_no_p_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    // GNU `Fyes_or_no_p` dynamically binds `real-this-command` around the
    // recursive minibuffer (src/fns.c).  The minibuffer command loop is
    // allowed to assign this variable while it dispatches self-insert and
    // `exit-minibuffer`, but those assignments belong to the nested command
    // context.  Keeping the boundary in the specpdl means every return path --
    // success, quit, signal, or future retry error -- restores the caller.
    let specpdl_count = eval.specpdl.len();
    let caller = eval.eval_symbol("real-this-command").unwrap_or(Value::NIL);
    eval.try_specbind_or_unwind_to(specpdl_count, intern("real-this-command"), caller)?;
    let result = finish_yes_or_no_p_with_minibuffer(args, |minibuffer_args| {
        finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    });
    eval.unbind_to_with_result(specpdl_count, result)
}

pub(crate) fn finish_yes_or_no_p_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let prompt_ls = if args[0].is_string() {
        args[0].as_lisp_string().expect("checked string").clone()
    } else {
        crate::heap_types::LispString::from_unibyte(Vec::new())
    };
    // Build the prompt exactly like GNU `Fyes_or_no_p` (fns.c): append
    // `yes-or-no-prompt` ("(yes or no) "), preceded by a single space only when
    // the prompt does not already end in whitespace.
    let ends_in_blank = prompt_ls
        .as_bytes()
        .last()
        .is_some_and(|&b| b == b' ' || b == b'\t');
    let suffix: &[u8] = if ends_in_blank {
        b"(yes or no) "
    } else {
        b" (yes or no) "
    };
    let full_prompt = prompt_ls.concat(&crate::heap_types::LispString::from_unibyte(
        suffix.to_vec(),
    ));
    loop {
        let result = read_from_minibuffer(&[Value::heap_string(full_prompt.clone())])?;
        if result.is_string() {
            let answer = result.as_lisp_string().expect("checked string");
            // The valid answers are ASCII ("yes"/"no"); decode lossily to compare.
            match crate::emacs_core::emacs_char::to_utf8_lossy(answer.as_bytes()).trim() {
                "yes" => return Ok(Value::T),
                "no" => return Ok(Value::NIL),
                _ => continue,
            }
        }
    }
}

pub(crate) fn builtin_yes_or_no_p_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    validate_yes_or_no_p_args(args)?;

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            // Batch/noninteractive: GNU's `read_minibuf_noninteractive` (minibuf.c)
            // writes the prompt to stdout and reads the answer from stdin. Mirror
            // that — including the yes/no re-prompt loop — instead of failing before
            // the prompt is ever shown, so batch `yes-or-no-p` emits the prompt
            // exactly like GNU (and still signals end-of-file on empty stdin).
            finish_yes_or_no_p_with_minibuffer(args, |minibuffer_args| {
                let prompt = minibuffer_args[0]
                    .as_lisp_string()
                    .expect("yes-or-no-p minibuffer prompt is a string");
                read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                    prompt.as_bytes(),
                ))
            })
            .map(Some)
        }
    }
}

fn validate_yes_or_no_p_args(args: &[Value]) -> Result<(), Flow> {
    expect_args("yes-or-no-p", args, 1)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    Ok(())
}

fn yes_or_no_p_dialog_result(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    if !yes_or_no_p_should_use_dialog(eval) {
        return Ok(None);
    }
    let menu = Value::cons(
        args[0],
        Value::list(vec![
            Value::cons(Value::string("Yes"), Value::T),
            Value::cons(Value::string("No"), Value::NIL),
        ]),
    );
    super::display::builtin_x_popup_dialog(eval, vec![Value::T, menu, Value::NIL]).map(Some)
}

fn yes_or_no_p_use_short_answers(eval: &super::eval::Context) -> bool {
    eval.obarray
        .symbol_value("use-short-answers")
        .is_some_and(|v| v.is_truthy())
}

fn yes_or_no_p_should_use_dialog(runtime: &impl KeyboardInputRuntime) -> bool {
    let last_input_event = runtime.symbol_value_or_nil("last-input-event");
    if last_input_event.is_nil() || !runtime.symbol_value_or_nil("use-dialog-box").is_truthy() {
        return false;
    }

    let last_nonmenu_event = runtime.symbol_value_or_nil("last-nonmenu-event");
    let from_tty_menu = runtime.symbol_value_or_nil("from--tty-menu-p");
    last_nonmenu_event.is_cons()
        || (last_nonmenu_event.is_nil() && last_input_event.is_cons())
        || (from_tty_menu.is_truthy() && from_tty_menu.as_symbol_name() != Some("unbound"))
}

// ---------------------------------------------------------------------------
// 17. read-char
// ---------------------------------------------------------------------------

/// `(read-char &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Read a character from the command input (keyboard or macro).
/// In batch mode, checks `unread-command-events` and returns nil if empty.
/// In interactive mode, blocks on the input channel via `read_char()`.
/// GNU `read_char` (keyboard.c) displays a non-nil string PROMPT in the echo
/// area for the duration of the read. Mirror that so prompts such as
/// `perform-replace`'s `(read-key "Query replacing ...: (? for help) ")` are
/// visible while waiting for the key. A nil/omitted or empty prompt shows
/// nothing (GNU only echoes a non-empty string prompt).
pub(crate) fn display_read_prompt(eval: &mut super::eval::Context, args: &[Value]) {
    if let Some(prompt) = args.first().and_then(|v| eval.lisp_string(*v))
        && !prompt.is_empty()
    {
        eval.set_current_message(Some(prompt.clone()));
    }
}

pub(crate) fn builtin_read_char(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_char_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_char_in_eval(eval, &args)
}

pub(crate) fn finish_read_char_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_char_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_char_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = tty_input_decoding_from_read_args(args);
    let Some(event) = runtime.read_char_with_timeout(read_plan.timeout(), tty_input_decoding)?
    else {
        return Ok(Value::NIL);
    };
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());
    if let Some(n) = event_to_int(&event) {
        if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
            runtime.set_read_command_keys(vec![event]);
        }
        return Ok(Value::fixnum(n));
    }
    runtime.replace_unread_command_event_with_singleton(event);
    runtime.record_input_event(event);
    Err(non_character_input_event_error())
}

/// `(read-key &optional PROMPT)`
///
/// Read a key from the command input.
/// In batch mode, returns next `unread-command-events` event, else nil.
/// In interactive mode, blocks on the input channel via `read_char()`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_read_key(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("read-key"), Value::fixnum(args.len() as i64)],
        ));
    }
    expect_optional_prompt_string(&args)?;

    // 1. Check unread-command-events first
    if let Some(event) = eval.pop_unread_command_event() {
        eval.record_nonmenu_input_event(event);
        eval.set_read_command_keys(vec![event]);
        if let Some(n) = event_to_int(&event) {
            return Ok(Value::fixnum(n));
        }
        return Ok(event);
    }

    // 2. Interactive mode: block on input channel
    if eval.input_rx.is_some() {
        let event = eval.read_char()?;
        eval.record_nonmenu_input_event(event);
        eval.set_read_command_keys(vec![event]);
        if let Some(n) = event_to_int(&event) {
            return Ok(Value::fixnum(n));
        }
        return Ok(event);
    }

    // 3. Batch mode: no input
    eval.clear_read_command_keys();
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 18. read-key-sequence
// ---------------------------------------------------------------------------

/// `(read-key-sequence PROMPT &optional ...)`
///
/// Read a sequence of keystrokes that forms a complete key binding.
/// In batch mode, consumes one queued event. In interactive mode, uses the
/// evaluator's `read_key_sequence()` to accumulate keys through prefix keymaps.
pub(crate) fn builtin_read_key_sequence(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if let Some(value) = builtin_read_key_sequence_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_key_sequence_in_eval(eval, &args)
}

pub(crate) fn finish_read_key_sequence_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_key_sequence_interactive_in_runtime(eval, read_key_sequence_options_from_args(args))
}

pub(crate) fn finish_read_key_sequence_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    options: crate::keyboard::ReadKeySequenceOptions,
) -> EvalResult {
    let (keys, _binding) = runtime.read_key_sequence_blocking(options)?;
    let mut chars_only = true;
    let mut s = String::new();
    for k in &keys {
        if let Some(c) = event_to_char(k) {
            s.push(c);
        } else {
            chars_only = false;
            break;
        }
    }
    if chars_only {
        return Ok(Value::string(s));
    }
    Ok(Value::vector(keys))
}

/// `(read-key-sequence-vector PROMPT)`
///
/// Batch mode: returns next `unread-command-events` event as a single-element
/// vector when present, otherwise an empty vector.
pub(crate) fn builtin_read_key_sequence_vector(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if let Some(value) = builtin_read_key_sequence_vector_in_runtime(eval, &args)? {
        return Ok(value);
    }
    finish_read_key_sequence_vector_interactive_in_runtime(
        eval,
        read_key_sequence_options_from_args(&args),
    )
}

pub(crate) fn finish_read_key_sequence_vector_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    options: crate::keyboard::ReadKeySequenceOptions,
) -> EvalResult {
    let (keys, _binding) = runtime.read_key_sequence_blocking(options)?;
    Ok(Value::vector(keys))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/minibuffer_teardown.rs"]
mod minibuffer_teardown_tests;
#[cfg(test)]
#[path = "tests/raw_bytes.rs"]
mod raw_bytes_tests;
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
