//! Signed display authority fence (peer 19:36Z follow-up).
//!
//! The three DisplayFrameExt test fixtures are cfg-gated trait methods used
//! by the re-homed load/reader oracles and by display/eval tests. Each has
//! one AUTHORITY-ITEMS row; these delegates are the signed fence path. The
//! trait methods keep their bodies and cfg gate unchanged (the N17 extension
//! trait cannot expose pub(crate) inherent methods on the eval-owned Frame).

use crate::window::geometry;
use crate::window::{Frame, WindowDisplaySnapshot};
use crate::window::{
    commit_redisplay_cache_for_test as commit_redisplay_cache_free,
    prepare_and_activate_display_presentation_for_test as prepare_and_activate_free,
    replace_redisplay_cache_for_test as replace_redisplay_cache_free,
};

pub fn commit_redisplay_cache_for_test(frame: &mut Frame, snapshots: Vec<WindowDisplaySnapshot>) {
    commit_redisplay_cache_free(frame, snapshots);
}

pub fn prepare_and_activate_display_presentation_for_test(
    frame: &mut Frame,
    presentation: geometry::PresentationId,
    snapshots: Vec<WindowDisplaySnapshot>,
) -> Result<(), geometry::PresentationPrepareError> {
    prepare_and_activate_free(frame, presentation, snapshots)
}

pub fn replace_redisplay_cache_for_test(frame: &mut Frame, snapshots: Vec<WindowDisplaySnapshot>) {
    replace_redisplay_cache_free(frame, snapshots);
}

/// FENCE (jitcall eval_test re-home, peer 20:55Z): thread-local redisplay
/// idle-skip override (test-mode write). The override itself stays private to
/// its defining module; this delegate is the signed AUTHORITY row route.
pub fn set_redisplay_idle_skip_for_test(enabled: Option<bool>) {
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(enabled);
}

/// FENCE (display upper-layer re-home, lead S1): build and commit one
/// window-scroll update from plain field values. `WindowScrollUpdate` stays
/// pub(crate); no handle crosses into core.
pub fn commit_window_scroll_for_test(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    frame: crate::window::FrameId,
    window: crate::window::WindowId,
    buffer: neovm_edit::buffer::BufferId,
    start: neovm_edit::buffer::LispCharPos1,
    point: neovm_edit::buffer::LispCharPos1,
    hidden_top_pixels: i32,
) -> Result<(), neovm_value::error::Flow> {
    crate::window::WindowScrollUpdate {
        frame,
        window,
        buffer,
        start,
        point,
        hidden_top_pixels,
    }
    .commit(eval)
}

/// FENCE (display upper-layer re-home, lead S1): run the boundary-mode
/// fontification entry point. The original stays private; this is its only
/// cross-crate route.
pub fn ensure_fontified_for_redisplay_with_mode_for_test(
    ctx: &mut neovm_eval::emacs_core::eval::Context,
    buf_id: neovm_edit::buffer::BufferId,
    from_char: i64,
    to_char: i64,
    mode: neovm_edit::emacs_core::textprop::FontificationBoundaryMode,
) -> Result<crate::emacs_core::xdisp::RedisplayFontificationOutcome, neovm_value::error::Flow> {
    crate::emacs_core::xdisp::ensure_fontified_for_redisplay_with_mode_for_test(
        ctx, buf_id, from_char, to_char, mode,
    )
}

/// FENCE (display upper-layer re-home, lead S1): the terminal handle reader
/// writes the TLS registry, so it is an AUTHORITY item; the original stays
/// pub(crate).
pub fn terminal_handle_value_for_id_for_test(id: u64) -> Option<neovm_value::value::Value> {
    crate::emacs_core::terminal::pure::terminal_handle_value_for_id(id)
}

/// FENCE (display upper-layer re-home, lead S1): `selected-terminal` writes
/// the TLS registry through the handle reader; the original stays pub(crate).
pub fn builtin_selected_terminal_for_test(
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::terminal::pure::builtin_selected_terminal(args)
}

/// FENCE (display upper-layer re-home, lead #767): the moved attempt-freshness
/// test forms a token copy whose source buffer is replaced by the live one.
/// That assignment is write authority over a widened display field, so it is
/// this fence, not a direct field write from core.
pub fn set_attempt_source_buffer_from_live_for_test(
    token: &mut crate::window::WindowLayoutAttemptFreshness,
) {
    token.source_buffer = token.live_buffer.clone();
}

/// Force the numeric mode-line padding policy for the re-homed min-width
/// tests. The original knob stays pub(crate) behind the xdisp re-export;
/// this fence is the signed cross-crate path.
pub fn force_mode_line_numeric_padding_for_test(value: Option<bool>) -> Option<bool> {
    crate::emacs_core::xdisp::force_for_test(value)
}

/// Scope a GC-root frame around a mode-line walk (moved prop-borrow tests).
pub fn with_root_scope<R>(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    operation: impl FnOnce(&mut neovm_eval::emacs_core::eval::Context) -> R,
) -> R {
    crate::emacs_core::xdisp::mode_line_walk::with_root_scope(eval, operation)
}

/// Keep the scratch GC roots alive while a moved test renders directly.
struct ScratchRootsGuard(crate::emacs_core::xdisp::mode_line_gc::ScratchRoots);

/// Push the scratch-root scope for the moved direct-render tests.
pub fn scratch_roots_for_test() -> impl Drop {
    ScratchRootsGuard(crate::emacs_core::xdisp::mode_line_gc::ScratchRoots::new())
}

impl Drop for ScratchRootsGuard {
    fn drop(&mut self) {}
}

/// Force the plain-field rendering path for the moved differential tests.
pub fn set_mode_line_plain_field_for_test(
    path: crate::emacs_core::xdisp::ModeLinePlainFieldPath,
) -> crate::emacs_core::xdisp::ModeLinePlainFieldPath {
    crate::emacs_core::xdisp::set_mode_line_plain_field_for_test(path)
}

/// Override the property-read policy for the moved differential tests.
pub fn set_mode_line_prop_borrow_for_test(enabled: Option<bool>) {
    crate::emacs_core::xdisp::set_mode_line_prop_borrow_for_test(enabled)
}

/// RAII policy guard used by the shared plain-field helper (one home).
struct PlainFieldGuard {
    previous: crate::emacs_core::xdisp::ModeLinePlainFieldPath,
}

impl PlainFieldGuard {
    pub fn set(
        path: crate::emacs_core::xdisp::ModeLinePlainFieldPath,
        properties: crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::PropertyRead,
    ) -> Self {
        let previous = set_mode_line_plain_field_for_test(path);
        match properties {
            crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::PropertyRead::Copied => {
                set_mode_line_prop_borrow_for_test(Some(false))
            }
            crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::PropertyRead::Borrowed => {
                set_mode_line_prop_borrow_for_test(Some(true))
            }
        }
        Self { previous }
    }
}

impl Drop for PlainFieldGuard {
    fn drop(&mut self) {
        set_mode_line_prop_borrow_for_test(None);
        set_mode_line_plain_field_for_test(self.previous);
    }
}

/// RAII guard over the plain-field policy knobs (moved tests).
pub fn plain_field_guard_for_test(
    path: crate::emacs_core::xdisp::ModeLinePlainFieldPath,
    properties: crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::PropertyRead,
) -> impl Drop {
    PlainFieldGuard::set(path, properties)
}

/// Append one display (min-width) run through the production formatter.
pub fn append_mode_line_percent_string_spec(
    result: &mut crate::emacs_core::xdisp::ModeLineRendered,
    spec: &str,
    props_at_percent: &[(neovm_value::value::Value, neovm_value::value::Value)],
    field_width: i64,
) {
    crate::emacs_core::xdisp::append_mode_line_percent_string_spec(
        result,
        spec,
        props_at_percent,
        field_width,
    )
}

/// Append a string value while preserving its properties (moved tests).
pub fn mode_line_rendered_append_string_value_preserving_props(
    rendered: &mut crate::emacs_core::xdisp::ModeLineRendered,
    value: &neovm_value::value::Value,
) {
    rendered.append_string_value_preserving_props(value)
}

/// Publish the rendered accumulator's scratch roots (moved tests).
pub fn mode_line_rendered_root_for_walk(rendered: &mut crate::emacs_core::xdisp::ModeLineRendered) {
    rendered.root_for_walk()
}

/// Construct the capturing frame-font fixture (moved font-size tests).
pub fn capturing_frame_font_display_host_for_test(
    requested_size: std::rc::Rc<std::cell::Cell<crate::emacs_core::display_host::FrameFontSize>>,
    realized: neovm_eval::emacs_core::eval::ResolvedFrameFont,
) -> crate::test_utils::xfaces_upper_layer_helpers::CapturingFrameFontDisplayHost {
    crate::test_utils::xfaces_upper_layer_helpers::CapturingFrameFontDisplayHost {
        requested_size,
        realized,
    }
}

/// Construct the frame-specific font fixture (moved font-size tests).
pub fn frame_specific_font_display_host_for_test(
    selected_frame: crate::window::FrameId,
    requests: std::rc::Rc<std::cell::RefCell<Vec<crate::window::FrameId>>>,
) -> crate::test_utils::xfaces_upper_layer_helpers::FrameSpecificFontDisplayHost {
    crate::test_utils::xfaces_upper_layer_helpers::FrameSpecificFontDisplayHost {
        selected_frame,
        requests,
    }
}

/// Construct the live-frame font fixture (moved builtins tests).
pub fn live_frame_font_display_host_for_test(
    realized: Option<neovm_eval::emacs_core::eval::ResolvedFrameFont>,
) -> crate::test_utils::xfaces_upper_layer_helpers::LiveFrameFontDisplayHost {
    crate::test_utils::xfaces_upper_layer_helpers::LiveFrameFontDisplayHost { realized }
}

/// Finalize `format-mode-line` in an evaluator Context (moved tests).
pub fn finish_format_mode_line_in_eval_for_test(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    args: &[neovm_value::value::Value],
) -> neovm_value::error::EvalResult {
    crate::emacs_core::xdisp::finish_format_mode_line_in_eval(eval, args)
}

/// Finalize `format-mode-line` in the split state (moved tests).
pub fn finish_format_mode_line_in_state_with_eval_for_test(
    vars: neovm_eval::emacs_core::eval::VariableView<'_>,
    dynamic: &[neovm_value::value::OrderedRuntimeBindingMap],
    frames: &crate::window::FrameManager,
    buffers: &mut neovm_edit::buffer::BufferManager,
    processes: &neovm_eval::emacs_core::eval::data::process_state::ProcessManager,
    args: &[neovm_value::value::Value],
    eval_form: impl FnMut(
        &neovm_value::value::Value,
        &neovm_edit::buffer::BufferManager,
    ) -> Result<neovm_value::value::Value, neovm_value::error::Flow>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::xdisp::finish_format_mode_line_in_state_with_eval(
        vars, dynamic, frames, buffers, processes, args, eval_form,
    )
}

/// Observer-taking fontification walk (moved demand tests).
pub fn ensure_fontified_for_redisplay_inner_for_test(
    ctx: &mut neovm_eval::emacs_core::eval::Context,
    buf_id: neovm_edit::buffer::BufferId,
    from_char: i64,
    to_char: i64,
    mode: neovm_edit::emacs_core::textprop::FontificationBoundaryMode,
    observer: &mut crate::emacs_core::xdisp::RedisplayFontificationDemand,
) -> Result<crate::emacs_core::xdisp::RedisplayFontificationOutcome, neovm_value::error::Flow> {
    crate::emacs_core::xdisp::ensure_fontified_for_redisplay_inner_for_test(
        ctx, buf_id, from_char, to_char, mode, observer,
    )
}

/// Swap the mode-line source-slice test selector (moved prop-borrow tests).
pub fn replace_mode_line_source_slice_test_mode_for_test(
    mode: crate::emacs_core::xdisp::ModeLineSourceSliceTestMode,
) -> crate::emacs_core::xdisp::ModeLineSourceSliceTestMode {
    crate::emacs_core::xdisp::replace_mode_line_source_slice_test_mode(mode)
}

/// Restore the mode-line source-slice test selector (moved prop-borrow tests).
pub fn restore_mode_line_source_slice_test_mode_for_test(
    mode: crate::emacs_core::xdisp::ModeLineSourceSliceTestMode,
) {
    crate::emacs_core::xdisp::restore_mode_line_source_slice_test_mode(mode)
}

/// Swap the property-borrow override and return the previous value.
pub fn replace_mode_line_prop_borrow_for_test(enabled: Option<bool>) -> Option<bool> {
    crate::emacs_core::xdisp::replace_mode_line_prop_borrow_for_test(enabled)
}

/// Snapshot the alternative-font registry without exposing the lock (moved gc test).
pub fn alternative_font_registry_alist_for_test() -> Vec<(
    neovm_value::heap_types::LispString,
    Vec<neovm_value::heap_types::LispString>,
)> {
    crate::emacs_core::font::alternative_font_registry_alist()
        .read()
        .unwrap()
        .iter()
        .map(|(name, aliases)| {
            (
                name.to_lisp_string(),
                aliases.iter().map(|alias| alias.to_lisp_string()).collect(),
            )
        })
        .collect()
}

/// GNU `risky-local-variable` symbol id used by the moved dynamic-symbol test.
pub fn mode_line_risky_local_variable_id_for_test() -> neovm_value::intern::SymId {
    crate::emacs_core::xdisp::risky_local_variable_symbol_id()
}

/// Record one display (min-width) transition on a rendered accumulator.
pub fn mode_line_rendered_note_display_min_width_transition(
    rendered: &mut crate::emacs_core::xdisp::ModeLineRendered,
    props: neovm_value::value::Value,
) {
    rendered.note_display_min_width_transition(props)
}

/// Construct the recording terminal host for the moved idle-redisplay test.
pub fn recording_terminal_host_for_test(
    log: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
) -> crate::test_utils::terminal_test_upper_layer_helpers::RecordingTerminalHost {
    crate::test_utils::terminal_test_upper_layer_helpers::RecordingTerminalHost { log }
}
