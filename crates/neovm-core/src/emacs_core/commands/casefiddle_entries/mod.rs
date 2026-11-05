//! Case-conversion Lisp entries that stay in the core command layer
//! (stage-3b R0).
//!
//! The region/word casing entries bind evaluator state through
//! `casify_replace_current_buffer_region`: the current-buffer case table
//! (raw-obarray legacy writes, green/tagged-heap bookkeeping), modification
//! hooks, and a borrowed text-property policy held across the buffer
//! mutation.  Under R0 the entries stay core-side; the case engine
//! (`casify_text_with_extents`, the subsidiary tables, the string helpers)
//! stays in the text layer and is called downward.

use crate::emacs_core::casefiddle::{
    CaseAction, capitalize_with_word_pred, case_encoding_for_current_buffer, casify_region_text,
    changed_char_span, expect_int, expect_min_max_args, noncontiguous_case_regions,
    upcase_initials_with_word_pred, upcase_word_predicate,
};
use crate::emacs_core::casetab_entries::case_table_override_for_current_buffer;
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use neovm_value::value::*;

fn casify_region_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    name: &str,
    action: CaseAction,
) -> EvalResult {
    expect_min_max_args(name, &args, 2, 3)?;
    let regions = if args.get(2).is_some_and(|value| !value.is_nil()) {
        noncontiguous_case_regions(eval)?
    } else {
        vec![super::position::LispRegionArgs::from_values(
            &eval.edit.buffers,
            args[0],
            args[1],
        )?]
    };
    for region in regions {
        casify_replace_current_buffer_region(eval, region, action)?;
    }
    Ok(Value::NIL)
}

/// Run GNU's modification preparation before reading the casing context or
/// source. Character arguments survive hooks; byte positions and per-character
/// replacement extents are measured together afterwards, without Lisp callbacks
/// between measurement and storage execution (casefiddle.c:528-583).
fn casify_replace_current_buffer_region(
    eval: &mut super::eval::Context,
    region: super::position::LispRegionArgs,
    action: CaseAction,
) -> Result<crate::buffer::CharPos0, Flow> {
    let (buffer_id, byte_range) = {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        let byte_range = region.accessible_byte_range(buf)?;
        if byte_range.is_empty() {
            return Ok(buf
                .emacs_byte_pos_to_lisp_char_pos(byte_range.end())
                .to_char_pos());
        }
        if super::editfns::buffer_read_only_active_in_state(eval.variables(), &[], buf) {
            return Err(signal(
                LispCondition::BufferReadOnly,
                vec![buf.name_value()],
            ));
        }
        (buf.id(), byte_range)
    };
    crate::emacs_core::textprop::verify_text_read_only_emacs_byte_range_in_state(
        eval.variables(),
        &eval.edit.buffers,
        buffer_id,
        byte_range,
    )?;
    let before = super::editfns::text_change_for_unchanged_extent_in_manager(
        &eval.edit.buffers,
        buffer_id,
        byte_range,
    )?;
    // GNU modify_text precedes prepare_casing_context (casefiddle.c:540-541).
    // A before-change hook can change text, encoding, syntax and case tables.
    super::editfns::signal_before_text_change(eval, before)?;
    let (buffer_id, byte_range, text) = {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        let chars = before.old_range().char_range();
        if chars.end() > buf.total_char_end_pos() {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![
                    Value::make_buffer(buf.id()),
                    Value::fixnum(chars.start().to_lisp().as_i64()),
                    Value::fixnum(chars.end().to_lisp().as_i64()),
                ],
            ));
        }
        // GNU validates the original accessible region only before hooks.
        // CHAR_TO_BYTE and make_buffer_string then use physical bounds even
        // if the hook narrows the end (casefiddle.c:534-557; editfns.c:1561-1566).
        // Its property copy still validates the original starting position
        // (editfns.c:1620-1624), so narrowing past start signals before editing.
        if chars.start() < buf.point_min_char_pos() || chars.start() > buf.point_max_char_pos() {
            crate::emacs_core::textprop::builtin_text_properties_at_in_buffers(
                &eval.edit.buffers,
                &[Value::fixnum(chars.start().to_lisp().as_i64())],
            )?;
        }
        let byte_range = buf.edit_range_for_char_range(chars).byte_range();
        (
            buf.id(),
            byte_range,
            buf.buffer_substring_lisp_string_range(byte_range),
        )
    };
    let casetab = case_table_override_for_current_buffer(eval)?;
    let replacement = match action {
        CaseAction::Up => {
            let is_word = upcase_word_predicate(eval, &casetab);
            casify_region_text(&text, action, is_word, &casetab)
        }
        CaseAction::Down | CaseAction::Capitalize | CaseAction::Initials => {
            let is_word = crate::emacs_core::syntax::casing_word_predicate(eval);
            casify_region_text(&text, action, is_word, &casetab)
        }
    };
    let change = super::editfns::text_change_for_lisp_string_replacement_in_manager(
        &eval.edit.buffers,
        buffer_id,
        byte_range,
        replacement.text(),
    )?;
    let end = change
        .old_range()
        .char_start()
        .add_len(change.new_extent().chars());
    // GNU records casing undo even for an unchanged nonempty region. The
    // before hook runs for that case, but the after hook requires changed text.
    // GNU sticky inheritance controls are read only after modification hooks.
    // Plain/no-expansion casing needs no policy lookup or interval allocation.
    let controls = (!replacement.expansions().is_empty()
        && eval
            .edit
            .buffers
            .get(buffer_id)
            .is_some_and(|buf| !buf.text_props_is_empty()))
    .then(|| crate::buffer::text_props::CasingPropertyControls::for_host(eval));
    let (obarray, buffers) = eval.obarray_and_buffers_mut();
    let properties =
        crate::buffer::text_props::CasingPropertyMode::from_controls(obarray, controls);
    buffers.casify_replace_buffer_region_with_expansions(
        buffer_id,
        byte_range,
        replacement.text(),
        replacement.expansions(),
        replacement.storage_shape(),
        &properties,
    )?;
    if replacement.text().as_bytes() != text.as_bytes() {
        if let Some(span) = changed_char_span(&text, &replacement) {
            // GNU casefiddle.c:475-525 retains the first source position and
            // last output end; :570 subtracts character growth from the old length.
            super::editfns::signal_after_change_chars(
                eval,
                change.old_range().char_start().add_len(span.offset()),
                span.old_len(),
                span.new_len(),
            )?;
        }
    }
    Ok(end)
}

fn casify_word_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    name: &str,
    action: CaseAction,
) -> EvalResult {
    expect_args(name, &args, 1)?;
    let n = expect_int(&args[0])?;
    // GNU scans words before modification preparation (casefiddle.c:661-668).
    let honor = crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval);
    let target = crate::emacs_core::syntax::forward_word_destination(eval, n, honor);
    let (start, end) = {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        (
            buf.point_lisp_char_pos(),
            buf.emacs_byte_pos_to_lisp_char_pos(target),
        )
    };
    let region = super::position::LispRegionArgs::from_values(
        &eval.edit.buffers,
        Value::fixnum(start.as_i64()),
        Value::fixnum(end.as_i64()),
    )?;
    let end = casify_replace_current_buffer_region(eval, region, action)?;
    // GNU SET_PT converts casify_region's character endpoint after the after
    // hook, which can change byte widths again (casefiddle.c:668-669). Negative
    // word arguments retain the greater endpoint, as region validation orders it.
    if let Some(buf) = eval.edit.buffers.current_buffer() {
        let id = buf.id();
        let point = buf.char_pos_to_emacs_byte_pos_clamped(end);
        let _ = eval.edit.buffers.goto_buffer_emacs_byte_pos(id, point);
    }
    Ok(Value::NIL)
}

/// Dispatched form: word boundaries follow the current buffer's syntax table
/// (honoring `case-symbols-as-words` and any `set-case-syntax-pair` word
/// syntax), and case mapping follows the current buffer's case table.
pub(crate) fn builtin_capitalize_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(eval);
    let casetab = case_table_override_for_current_buffer(eval)?;
    capitalize_with_word_pred(
        args,
        is_word,
        &casetab,
        case_encoding_for_current_buffer(eval),
    )
}

/// Dispatched form: word boundaries follow the current buffer's syntax table.
pub(crate) fn builtin_upcase_initials_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(eval);
    let casetab = case_table_override_for_current_buffer(eval)?;
    upcase_initials_with_word_pred(
        args,
        is_word,
        &casetab,
        case_encoding_for_current_buffer(eval),
    )
}

pub(crate) fn builtin_downcase_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_region_in_state(ctx, args, "downcase-region", CaseAction::Down)
}

pub(crate) fn builtin_upcase_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_region_in_state(ctx, args, "upcase-region", CaseAction::Up)
}

pub(crate) fn builtin_capitalize_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_region_in_state(ctx, args, "capitalize-region", CaseAction::Capitalize)
}

pub(crate) fn builtin_upcase_initials_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_region_in_state(ctx, args, "upcase-initials-region", CaseAction::Initials)
}

pub(crate) fn builtin_downcase_word(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_word_in_state(ctx, args, "downcase-word", CaseAction::Down)
}

pub(crate) fn builtin_upcase_word(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    casify_word_in_state(ctx, args, "upcase-word", CaseAction::Up)
}

pub(crate) fn builtin_capitalize_word(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    casify_word_in_state(ctx, args, "capitalize-word", CaseAction::Capitalize)
}
