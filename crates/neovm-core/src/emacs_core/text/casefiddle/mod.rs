//! Case conversion and character builtins.
//!
//! Implements `capitalize`, `upcase-initials`, and `char-resolve-modifiers`.

use super::casetab::{CaseMap, CaseTableOverride};
use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::buffer::{EmacsBytePos, EmacsByteRange};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use crate::emacs_core::value::ValueKind;
use crate::heap_types::LispString;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

// ---------------------------------------------------------------------------
// Character helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_META: i64 = 0x8000000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_CTL: i64 = 0x4000000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_SHIFT: i64 = 0x2000000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_HYPER: i64 = 0x1000000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_SUPER: i64 = 0x0800000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_ALT: i64 = 0x0400000;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const CHAR_MODIFIER_MASK: i64 =
    CHAR_META | CHAR_CTL | CHAR_SHIFT | CHAR_HYPER | CHAR_SUPER | CHAR_ALT;

/// Convert a character code to a Rust char (if it's a valid Unicode scalar value).
fn code_to_char(code: i64) -> Option<char> {
    if (0..=0x10FFFF).contains(&code) {
        char::from_u32(code as u32)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Case conversion helpers
// ---------------------------------------------------------------------------

/// Uppercase a single character code, consulting the per-buffer case table
/// first (GNU `upcase`), then falling through to the hardwired mapping.
fn upcase_char_override(code: i64, casetab: &CaseTableOverride) -> i64 {
    casetab
        .map(CaseMap::Up, code)
        .unwrap_or_else(|| upcase_char(code))
}

/// Uppercase a single character code, returning the new code.
fn upcase_char(code: i64) -> i64 {
    if preserve_casefiddle_upcase_payload(code) {
        return code;
    }
    match code {
        223 => return 7838,
        452 | 497 => return code + 1,
        454 | 457 | 460 | 499 => return code - 1,
        455 | 458 => return code + 1,
        8064..=8071 | 8080..=8087 | 8096..=8103 => return code + 8,
        8115 | 8131 | 8179 => return code + 9,
        _ => {}
    }
    match code_to_char(code) {
        Some(c) => {
            let mut upper = c.to_uppercase();
            // to_uppercase() may yield multiple chars (e.g. German eszett);
            // take only the first to stay consistent with Emacs behavior.
            upper.next().map(|u| u as i64).unwrap_or(code)
        }
        None => code,
    }
}

fn preserve_casefiddle_upcase_payload(code: i64) -> bool {
    matches!(
        code,
        329
            | 411
            | 453
            | 456
            | 459
            | 496
            | 498
            | 612
            | 912
            | 944
            | 1415
            | 4304..=4346
            | 4349..=4351
            | 7306
            | 7830..=7834
            | 8016
            | 8018
            | 8020
            | 8022
            | 8072..=8079
            | 8088..=8095
            | 8104..=8111
            | 8114
            | 8116
            | 8118..=8119
            | 8124
            | 8130
            | 8132
            | 8134..=8135
            | 8140
            | 8146..=8147
            | 8150..=8151
            | 8162..=8164
            | 8166..=8167
            | 8178
            | 8180
            | 8182..=8183
            | 8188
            | 42957
            | 42959
            | 42963
            | 42965
            | 42971
            | 64256..=64262
            | 64275..=64279
            | 68976..=68997
            | 93883..=93907
    )
}

fn titlecase_from_uppercase_expansion(expansion: &[char]) -> String {
    let mut result = String::new();
    let mut seen_cased = false;

    for uc in expansion {
        let is_cased = uc.is_uppercase() || uc.is_lowercase();
        if !seen_cased {
            result.push(*uc);
            if is_cased {
                seen_cased = true;
            }
            continue;
        }

        if is_cased {
            for lc in uc.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(*uc);
        }
    }

    result
}

fn titlecase_combining_iota_override(code: i64) -> Option<&'static str> {
    match code {
        8114 => Some("\u{1FBA}\u{0345}"),
        8116 => Some("\u{0386}\u{0345}"),
        8119 => Some("\u{0391}\u{0342}\u{0345}"),
        8130 => Some("\u{1FCA}\u{0345}"),
        8132 => Some("\u{0389}\u{0345}"),
        8135 => Some("\u{0397}\u{0342}\u{0345}"),
        8178 => Some("\u{1FFA}\u{0345}"),
        8180 => Some("\u{038F}\u{0345}"),
        8183 => Some("\u{03A9}\u{0342}\u{0345}"),
        _ => None,
    }
}

fn titlecase_uses_precomposed_upcase(code: i64) -> bool {
    matches!(
        code,
        8064..=8071
            | 8072..=8111
            | 8115
            | 8124
            | 8131
            | 8140
            | 8179
            | 8188
    )
}

fn titlecase_word_initial(c: char) -> String {
    let code = c as i64;
    if let Some(explicit) = titlecase_combining_iota_override(code) {
        return explicit.to_string();
    }

    let expansion: Vec<char> = c.to_uppercase().collect();
    if expansion.len() > 1 && !titlecase_uses_precomposed_upcase(code) {
        return titlecase_from_uppercase_expansion(&expansion);
    }

    if let Some(mapped) = code_to_char(upcase_char(code)) {
        mapped.to_string()
    } else {
        c.to_uppercase().collect()
    }
}

fn push_multibyte_char_code(out: &mut Vec<u8>, code: u32) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
    out.extend_from_slice(&buf[..len]);
}

fn push_multibyte_chars(out: &mut Vec<u8>, chars: impl IntoIterator<Item = char>) {
    for ch in chars {
        push_multibyte_char_code(out, ch as u32);
    }
}

/// Word-boundary predicate over the *standard* syntax table, for the pure/test
/// casing forms that run without a current buffer. Mirrors GNU's `Sword` test
/// against the standard syntax table.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn standard_word_predicate(code: u32) -> bool {
    crate::emacs_core::syntax::standard_syntax_class_for_code(code)
        == crate::emacs_core::syntax::SyntaxClass::Word
}

fn downcase_lisp_string_emacs_compat(
    text: &LispString,
    is_word: impl Fn(u32) -> bool,
    casetab: &CaseTableOverride,
) -> LispString {
    if !text.is_multibyte() {
        let bytes = text
            .as_bytes()
            .iter()
            .map(|&byte| {
                casetab
                    .map(CaseMap::Down, byte as i64)
                    .map(|m| m as u8)
                    .unwrap_or_else(|| byte.to_ascii_lowercase())
            })
            .collect();
        return LispString::from_unibyte(bytes);
    }

    // Greek capital sigma down-cases to its final form ς at the end of a word
    // (GNU `casefiddle.c` `case_character`).
    const GREEK_CAPITAL_SIGMA: u32 = 0x03A3;
    const GREEK_SMALL_SIGMA: u32 = 0x03C3;
    const GREEK_SMALL_FINAL_SIGMA: u32 = 0x03C2;

    let codes = super::builtins::lisp_string_char_codes(text);
    let mut out = Vec::with_capacity(text.sbytes());
    let mut prev_word = false;
    for (i, &code) in codes.iter().enumerate() {
        let code_i64 = code as i64;
        // GNU `downcase` consults the per-buffer case table first.
        if let Some(mapped) = casetab.map(CaseMap::Down, code_i64) {
            push_multibyte_char_code(&mut out, mapped as u32);
        } else if code == GREEK_CAPITAL_SIGMA {
            let next_word = codes.get(i + 1).is_some_and(|&next| is_word(next));
            push_multibyte_char_code(
                &mut out,
                if prev_word && !next_word {
                    GREEK_SMALL_FINAL_SIGMA
                } else {
                    GREEK_SMALL_SIGMA
                },
            );
        } else if code == 0x212A || preserve_downcase_case_string_payload(code_i64) {
            push_multibyte_char_code(&mut out, code);
        } else if let Some(ch) = code_to_char(code_i64) {
            push_multibyte_chars(&mut out, ch.to_lowercase());
        } else {
            push_multibyte_char_code(&mut out, code);
        }
        prev_word = is_word(code);
    }
    LispString::from_emacs_bytes(out)
}

fn upcase_lisp_string_emacs_compat(text: &LispString, casetab: &CaseTableOverride) -> LispString {
    if !text.is_multibyte() {
        let bytes = text
            .as_bytes()
            .iter()
            .map(|&byte| {
                casetab
                    .map(CaseMap::Up, byte as i64)
                    .map(|m| m as u8)
                    .unwrap_or_else(|| byte.to_ascii_uppercase())
            })
            .collect();
        return LispString::from_unibyte(bytes);
    }

    let mut out = Vec::with_capacity(text.sbytes());
    for code in super::builtins::lisp_string_char_codes(text) {
        let code_i64 = code as i64;
        // GNU `upcase` consults the per-buffer case table first.
        if let Some(mapped) = casetab.map(CaseMap::Up, code_i64) {
            push_multibyte_char_code(&mut out, mapped as u32);
            continue;
        }
        if code == 0x0131 || preserve_upcase_case_string_payload(code_i64) {
            push_multibyte_char_code(&mut out, code);
            continue;
        }
        if let Some(ch) = code_to_char(code_i64) {
            push_multibyte_chars(&mut out, ch.to_uppercase());
        } else {
            push_multibyte_char_code(&mut out, code);
        }
    }
    LispString::from_emacs_bytes(out)
}

fn capitalize_lisp_string(
    text: &LispString,
    is_word: impl Fn(u32) -> bool,
    casetab: &CaseTableOverride,
) -> LispString {
    if !text.is_multibyte() {
        let mut out = Vec::with_capacity(text.sbytes());
        let mut new_word = true;
        for &byte in text.as_bytes() {
            if is_word(byte as u32) {
                // Word-initial up-cases via the up table, the rest down-cases
                // via the down table (GNU `casify_object` CASE_CAPITALIZE).
                let which = if new_word { CaseMap::Up } else { CaseMap::Down };
                out.push(
                    casetab
                        .map(which, byte as i64)
                        .map(|m| m as u8)
                        .unwrap_or(if new_word {
                            byte.to_ascii_uppercase()
                        } else {
                            byte.to_ascii_lowercase()
                        }),
                );
                new_word = false;
            } else {
                out.push(byte);
                new_word = true;
            }
        }
        return LispString::from_unibyte(out);
    }

    let mut out = Vec::with_capacity(text.sbytes());
    let mut new_word = true;
    for code in super::builtins::lisp_string_char_codes(text) {
        let ch = code_to_char(code as i64);
        if is_word(code) {
            let which = if new_word { CaseMap::Up } else { CaseMap::Down };
            if let Some(mapped) = casetab.map(which, code as i64) {
                push_multibyte_char_code(&mut out, mapped as u32);
            } else {
                match ch {
                    Some(c) if new_word => {
                        push_multibyte_chars(&mut out, titlecase_word_initial(c).chars())
                    }
                    Some(c) => push_multibyte_chars(&mut out, c.to_lowercase()),
                    None => push_multibyte_char_code(&mut out, code),
                }
            }
            new_word = false;
        } else {
            push_multibyte_char_code(&mut out, code);
            new_word = true;
        }
    }
    LispString::from_emacs_bytes(out)
}

fn upcase_initials_lisp_string(
    text: &LispString,
    is_word: impl Fn(u32) -> bool,
    casetab: &CaseTableOverride,
) -> LispString {
    if !text.is_multibyte() {
        let mut out = Vec::with_capacity(text.sbytes());
        let mut new_word = true;
        for &byte in text.as_bytes() {
            if is_word(byte as u32) {
                // Only the word-initial is up-cased; the rest is left as-is.
                out.push(if new_word {
                    casetab
                        .map(CaseMap::Up, byte as i64)
                        .map(|m| m as u8)
                        .unwrap_or_else(|| byte.to_ascii_uppercase())
                } else {
                    byte
                });
                new_word = false;
            } else {
                out.push(byte);
                new_word = true;
            }
        }
        return LispString::from_unibyte(out);
    }

    let mut out = Vec::with_capacity(text.sbytes());
    let mut new_word = true;
    for code in super::builtins::lisp_string_char_codes(text) {
        let ch = code_to_char(code as i64);
        if is_word(code) {
            if new_word {
                if let Some(mapped) = casetab.map(CaseMap::Up, code as i64) {
                    push_multibyte_char_code(&mut out, mapped as u32);
                } else {
                    match ch {
                        Some(c) => {
                            push_multibyte_chars(&mut out, titlecase_word_initial(c).chars())
                        }
                        None => push_multibyte_char_code(&mut out, code),
                    }
                }
            } else {
                push_multibyte_char_code(&mut out, code);
            }
            new_word = false;
        } else {
            push_multibyte_char_code(&mut out, code);
            new_word = true;
        }
    }
    LispString::from_emacs_bytes(out)
}

fn preserve_downcase_case_string_payload(code: i64) -> bool {
    matches!(
        code,
        7305
            | 42955
            | 42956
            | 42958
            | 42962
            | 42964
            | 42970
            | 42972
            | 68944..=68965
            | 93856..=93880
    )
}

fn preserve_upcase_case_string_payload(code: i64) -> bool {
    matches!(
        code,
        411
            | 612
            | 7306
            | 42957
            | 42959
            | 42963
            | 42965
            | 42971
            | 68976..=68997
            | 93883..=93907
    )
}

fn noncontiguous_case_regions(
    eval: &mut super::eval::Context,
) -> Result<Vec<super::position::LispRegionArgs>, Flow> {
    let extractor = eval
        .eval_symbol("region-extract-function")
        .unwrap_or(Value::symbol("buffer-substring"));
    let bounds = eval.funcall_general(extractor, vec![Value::symbol("bounds")])?;
    let bounds_list = crate::emacs_core::value::list_to_vec(&bounds).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), bounds],
        )
    })?;
    bounds_list
        .into_iter()
        .map(|value| {
            if !value.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), value],
                ));
            }
            super::position::LispRegionArgs::from_values(
                &eval.buffers,
                value.cons_car(),
                value.cons_cdr(),
            )
        })
        .collect()
}

fn replace_current_buffer_region_in_buffers(
    eval: &mut super::eval::Context,
    byte_range: EmacsByteRange,
    replacement: &LispString,
    restore_point: bool,
) -> EvalResult {
    let (buffer_id, saved_pt) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            eval.buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?,
            buf.point_emacs_byte_pos(),
        )
    };
    super::fns::replace_buffer_emacs_byte_range_lisp_string(
        eval,
        buffer_id,
        byte_range,
        replacement,
    )?;
    if restore_point {
        let restore_pos = eval
            .buffers
            .get(buffer_id)
            .map(|buf| saved_pt.min(buf.accessible_emacs_byte_region().end()));
        if let Some(restore_pos) = restore_pos {
            let _ = eval
                .buffers
                .goto_buffer_emacs_byte_pos(buffer_id, restore_pos);
        }
    }
    Ok(Value::NIL)
}

fn casify_region_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    name: &str,
    transform: impl Fn(&LispString) -> LispString + Copy,
) -> EvalResult {
    expect_min_max_args(name, &args, 2, 3)?;

    let regions = if args.get(2).is_some_and(|value| !value.is_nil()) {
        noncontiguous_case_regions(eval)?
    } else {
        vec![super::position::LispRegionArgs::from_values(
            &eval.buffers,
            args[0],
            args[1],
        )?]
    };

    for region in regions {
        let (buffer_id, byte_range, text) = {
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let byte_range = region.accessible_byte_range(buf)?;
            if byte_range.is_empty() {
                continue;
            }
            if super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf) {
                return Err(signal(
                    LispCondition::BufferReadOnly,
                    vec![buf.name_value()],
                ));
            }
            let buffer_id = eval
                .buffers
                .current_buffer_id()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let text = buf.buffer_substring_lisp_string_range(byte_range);
            (buffer_id, byte_range, text)
        };

        // GNU `casify_region` calls `modify_text (start, end)`
        // (casefiddle.c:540) -> `prepare_to_modify_buffer_1` ->
        // `verify_interval_modification`, which signals `text-read-only` for any
        // read-only interval in [start,end) BEFORE any character is cased --
        // even when the conversion would be a no-op. The buffer-wide
        // `buffer-read-only` check above is GNU's `Fbarf_if_buffer_read_only`;
        // this is the text-property half it was missing.
        crate::emacs_core::textprop::verify_text_read_only_emacs_byte_range_in_state(
            &eval.obarray,
            &eval.buffers,
            buffer_id,
            byte_range,
        )?;

        // GNU `casify_region` (casefiddle.c) always `modify_text`s the range
        // and records `record_delete (start, ORIGINAL) + record_insert (start,
        // NEW_LEN)` even when no character changes, so the undo list keeps the
        // GNU shape `((START . END) (ORIGINAL . START) POINT ...)`.  Route the
        // edit through the casify-specific replace so the undo recording and
        // marker handling match GNU instead of the generic replace path.
        let replacement = transform(&text);
        // GNU fires after-change-functions only when a character actually
        // changed (e.g. `downcase-region` over already-lowercase text signals
        // before- but not after-change); before-change and the undo record
        // still happen for the no-op. Match that.
        let changed = replacement.as_bytes() != text.as_bytes();
        casify_replace_current_buffer_region(eval, byte_range, &replacement, changed)?;
    }

    Ok(Value::NIL)
}

/// Apply a case-region replacement to the current buffer, recording undo with
/// GNU `casify_region`'s shape.  Point and markers are preserved for a
/// same-length change, matching GNU's in-place `replace_range_2`.
fn casify_replace_current_buffer_region(
    eval: &mut super::eval::Context,
    byte_range: EmacsByteRange,
    replacement: &LispString,
    changed: bool,
) -> EvalResult {
    let buffer_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // GNU's `casify_region` (casefiddle.c) runs `modify_text` before and
    // `signal_after_change` after the edit; the earlier port kept only the
    // read-only checks and dropped the change hooks, so `upcase-region` &c.
    // mutated the buffer without firing before/after-change-functions -- which
    // `track-changes.el` flags as "Missing/incorrect calls to
    // before/after-change-functions" (issue #145). Bracket the casify-specific
    // replace exactly like the generic replace + the *word* case variants.
    let change = super::editfns::text_change_for_lisp_string_replacement_in_manager(
        &eval.buffers,
        buffer_id,
        byte_range,
        replacement,
    )?;
    super::editfns::signal_before_text_change(eval, change)?;
    eval.buffers
        .casify_replace_buffer_emacs_byte_range_lisp_string(buffer_id, byte_range, replacement);
    if changed {
        super::editfns::signal_after_text_change(eval, change)?;
    }
    Ok(Value::NIL)
}

fn casify_word_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    name: &str,
    transform: impl FnOnce(&LispString) -> LispString,
) -> EvalResult {
    expect_args(name, &args, 1)?;
    let n = expect_int(&args[0])?;

    // Honor `find-word-boundary-function-table` (subword/superword) for the
    // word boundary, mirroring GNU's forward-word; computed without moving point.
    let honor = crate::emacs_core::syntax::parse_sexp_lookup_properties_enabled(eval);
    let target = crate::emacs_core::syntax::forward_word_destination(eval, n, honor);
    let (byte_range, text, buffer_name, read_only) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let pt = buf.point_emacs_byte_pos();
        let byte_range = EmacsByteRange::ordered(pt, target);
        let text = buf.buffer_substring_lisp_string_range(byte_range);
        (
            byte_range,
            text,
            buf.name_value(),
            super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf),
        )
    };

    let replacement = transform(&text);
    let changed = replacement != text;
    if changed {
        if read_only {
            return Err(signal(LispCondition::BufferReadOnly, vec![buffer_name]));
        }
        replace_current_buffer_region_in_buffers(eval, byte_range, &replacement, false)?;
    }
    // GNU `casify_word` sets point to `casify_region(PT, farend)`, i.e. the
    // greater of point and the forward-word destination (the end of the cased
    // region), shifted by any length change. For a negative ARG this leaves
    // point at the original PT ("convert previous words but do not move").
    let _ = n;
    if let Some(id) = eval.buffers.current_buffer_id() {
        let delta = replacement.sbytes() as i64 - text.sbytes() as i64;
        let new_pt = (byte_range.end().get() as i64 + delta).max(0) as usize;
        let _ = eval
            .buffers
            .goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(new_pt));
    }
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(capitalize OBJ)` -- if OBJ is a string, capitalize the first letter
/// (uppercase first, lowercase rest).  If OBJ is a character, uppercase it.
fn capitalize_with_word_pred(
    args: Vec<Value>,
    is_word: impl Fn(u32) -> bool,
    casetab: &CaseTableOverride,
) -> EvalResult {
    expect_args("capitalize", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let source = args[0];
            let string = args[0].as_lisp_string().expect("string");
            let source_props = (!string.is_multibyte())
                .then(|| get_string_text_properties_table_for_value(source))
                .flatten();
            let result = Value::heap_string(capitalize_lisp_string(string, is_word, casetab));
            if let Some(table) = source_props {
                set_string_text_properties_table_for_value(result, table);
            }
            Ok(result)
        }
        ValueKind::Fixnum(c) => {
            let code = c;
            Ok(Value::fixnum(upcase_char_override(code, casetab)))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
    }
}

/// Pure form (used by tests); word boundaries follow the standard syntax table.
#[cfg(test)]
pub(crate) fn builtin_capitalize(args: Vec<Value>) -> EvalResult {
    capitalize_with_word_pred(args, standard_word_predicate, &CaseTableOverride::none())
}

/// Dispatched form: word boundaries follow the current buffer's syntax table
/// (honoring `case-symbols-as-words` and any `set-case-syntax-pair` word
/// syntax), and case mapping follows the current buffer's case table.
pub(crate) fn builtin_capitalize_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(eval);
    let casetab = CaseTableOverride::for_current_buffer(eval)?;
    capitalize_with_word_pred(args, is_word, &casetab)
}

/// `(upcase-initials OBJ)` -- uppercase the first letter of each word in
/// a string, leaving the rest unchanged.  For a char, uppercase it.
fn upcase_initials_with_word_pred(
    args: Vec<Value>,
    is_word: impl Fn(u32) -> bool,
    casetab: &CaseTableOverride,
) -> EvalResult {
    expect_args("upcase-initials", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let source = args[0];
            let string = args[0].as_lisp_string().expect("string");
            let source_props = (!string.is_multibyte())
                .then(|| get_string_text_properties_table_for_value(source))
                .flatten();
            let result = Value::heap_string(upcase_initials_lisp_string(string, is_word, casetab));
            if let Some(table) = source_props {
                set_string_text_properties_table_for_value(result, table);
            }
            Ok(result)
        }
        ValueKind::Fixnum(c) => {
            let code = c;
            Ok(Value::fixnum(upcase_char_override(code, casetab)))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
    }
}

/// Pure form (used by tests); word boundaries follow the standard syntax table.
#[cfg(test)]
pub(crate) fn builtin_upcase_initials(args: Vec<Value>) -> EvalResult {
    upcase_initials_with_word_pred(args, standard_word_predicate, &CaseTableOverride::none())
}

/// Dispatched form: word boundaries follow the current buffer's syntax table.
pub(crate) fn builtin_upcase_initials_in_state(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(eval);
    let casetab = CaseTableOverride::for_current_buffer(eval)?;
    upcase_initials_with_word_pred(args, is_word, &casetab)
}

/// Uppercase the first letter of each word, leaving the rest unchanged.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn upcase_initials_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut new_word = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if new_word {
                for u in titlecase_word_initial(c).chars() {
                    result.push(u);
                }
                new_word = false;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
            new_word = true;
        }
    }
    result
}

/// Classify the original matched text to decide whether to leave
/// REPLACEMENT as-is, upcase it entirely, or capitalize each word.
///
/// This is the backing logic for `replace-match`'s FIXEDCASE=nil
/// behavior. It mirrors GNU `src/search.c:2460-2525`, including the
/// `case-symbols-as-words` branch at search.c:2486/2495/2505.
///
/// `is_word_char` decides whether a character counts as a word
/// constituent for the "start of word" check. GNU consults the
/// buffer's syntax table (`SYNTAX(prevc) == Sword`) and, when
/// `case-symbols-as-words` is non-nil, also accepts `Ssymbol`. The
/// default closure below uses the standard syntax table defaults and
/// honors `case-symbols-as-words` via the supplied flag so that
/// callers who don't have a buffer handy still behave like GNU on
/// the standard table. Callers who do have a buffer handy should
/// pass a closure that consults `BVAR(current_buffer, syntax_table)`.
///
/// See audit findings #14 and #20 in `drafts/regex-search-audit.md`:
/// the old code used Rust's Unicode `is_alphanumeric()` and ignored
/// `case-symbols-as-words` entirely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplaceMatchCaseAction {
    NoChange,
    AllCaps,
    CapInitial,
}

/// Faithful (Emacs-byte) classification of the matched text's casing, read
/// directly from its `LispString` bytes with no storage-String round-trip.
pub(crate) fn replace_match_case_action_lisp_default(
    matched: &LispString,
) -> ReplaceMatchCaseAction {
    replace_match_case_action_lisp(matched, default_is_word_char)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn replace_match_case_action_with<F>(
    matched: &str,
    mut is_word_char: F,
) -> ReplaceMatchCaseAction
where
    F: FnMut(char) -> bool,
{
    let mut some_multiletter_word = false;
    let mut some_lowercase = false;
    let mut some_uppercase = false;
    let mut some_nonuppercase_initial = false;
    let mut prev_is_word = false;

    for ch in matched.chars() {
        if ch.is_lowercase() {
            some_lowercase = true;
            if prev_is_word {
                some_multiletter_word = true;
            } else {
                some_nonuppercase_initial = true;
            }
        } else if ch.is_uppercase() {
            some_uppercase = true;
            if prev_is_word {
                some_multiletter_word = true;
            }
        } else if !prev_is_word {
            some_nonuppercase_initial = true;
        }

        prev_is_word = is_word_char(ch);
    }

    if !some_lowercase && some_multiletter_word {
        ReplaceMatchCaseAction::AllCaps
    } else if !some_nonuppercase_initial && some_multiletter_word {
        ReplaceMatchCaseAction::CapInitial
    } else if !some_nonuppercase_initial && some_uppercase {
        ReplaceMatchCaseAction::AllCaps
    } else {
        ReplaceMatchCaseAction::NoChange
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn apply_replace_match_case(replacement: &str, matched: &str) -> String {
    apply_replace_match_case_with(replacement, matched, default_is_word_char)
}

/// Like `apply_replace_match_case`, but lets the caller supply the
/// predicate used for the "previous character is a word constituent"
/// check. Use this from paths that have a buffer syntax table in
/// scope so per-mode definitions of word constituents apply.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn apply_replace_match_case_with<F>(
    replacement: &str,
    matched: &str,
    is_word_char: F,
) -> String
where
    F: FnMut(char) -> bool,
{
    let case_action = replace_match_case_action_with(matched, is_word_char);

    match case_action {
        ReplaceMatchCaseAction::NoChange => replacement.to_string(),
        ReplaceMatchCaseAction::AllCaps => replacement.to_uppercase(),
        ReplaceMatchCaseAction::CapInitial => upcase_initials_string(replacement),
    }
}

/// Emacs-byte-aware variant of [`apply_replace_match_case`]. Operates over real
/// Emacs char codes (via `emacs_char::string_char`) and the LispString case
/// primitives, so eight-bit raw bytes and Private-Use-Area glyphs are analyzed
/// and cased faithfully instead of through the legacy PUA-sentinel storage form
/// (issue #131).
pub(crate) fn apply_replace_match_case_lisp(
    replacement: &LispString,
    matched: &LispString,
) -> LispString {
    apply_replace_match_case_lisp_with(replacement, matched, default_is_word_char)
}

/// Like [`apply_replace_match_case_lisp`], but lets the caller supply the
/// predicate used for the "previous character is a word constituent" check.
/// Use this from paths that have a buffer syntax table in scope so per-mode
/// definitions of word constituents apply. Mirrors
/// [`apply_replace_match_case_with`] but stays byte-faithful (issue #131).
pub(crate) fn apply_replace_match_case_lisp_with<F>(
    replacement: &LispString,
    matched: &LispString,
    is_word_char: F,
) -> LispString
where
    F: FnMut(char) -> bool,
{
    // replace-match's case-adjustment uses the standard case mapping (it has no
    // buffer case table in scope here); this matches prior behavior.
    let casetab = CaseTableOverride::none();
    match replace_match_case_action_lisp(matched, is_word_char) {
        ReplaceMatchCaseAction::NoChange => replacement.clone(),
        ReplaceMatchCaseAction::AllCaps => upcase_lisp_string_emacs_compat(replacement, &casetab),
        ReplaceMatchCaseAction::CapInitial => {
            // replace-match's case adjustment has no buffer syntax table in
            // scope, so word boundaries here follow the Unicode-alphanumeric
            // rule (matching prior behavior), independent of the buffer-aware
            // casing predicate used by capitalize/upcase-initials.
            upcase_initials_lisp_string(
                replacement,
                |code| char::from_u32(code).is_some_and(char::is_alphanumeric),
                &casetab,
            )
        }
    }
}

/// Like [`apply_replace_match_case_lisp_with`] but with caller-supplied
/// uppercase/lowercase predicates and case table, so a buffer's case table +
/// syntax table drive GNU's `Freplace_match` decisions (`UPPERCASEP` /
/// `LOWERCASEP` + `SYNTAX`). Used by the buffer replace-match path, which has
/// both tables in scope.
pub(crate) fn apply_replace_match_case_lisp_cased<W, U, L>(
    replacement: &LispString,
    matched: &LispString,
    is_word_char: W,
    is_upper: U,
    is_lower: L,
    casetab: &CaseTableOverride,
) -> LispString
where
    W: FnMut(char) -> bool,
    U: FnMut(char) -> bool,
    L: FnMut(char) -> bool,
{
    match replace_match_case_action_lisp_cased(matched, is_word_char, is_upper, is_lower) {
        ReplaceMatchCaseAction::NoChange => replacement.clone(),
        ReplaceMatchCaseAction::AllCaps => upcase_lisp_string_emacs_compat(replacement, casetab),
        ReplaceMatchCaseAction::CapInitial => upcase_initials_lisp_string(
            replacement,
            |code| char::from_u32(code).is_some_and(char::is_alphanumeric),
            casetab,
        ),
    }
}

/// Emacs-byte-aware counterpart of [`replace_match_case_action_with`]. Mirrors
/// the same GNU `src/search.c` decision logic, but iterates the matched text's
/// Emacs char codes; codes outside the Unicode scalar range (eight-bit raw
/// bytes, extended codes) are caseless, non-word constituents — matching GNU,
/// where raw bytes have no case and are not `Sword`.
pub(crate) fn replace_match_case_action_lisp<F>(
    matched: &LispString,
    is_word_char: F,
) -> ReplaceMatchCaseAction
where
    F: FnMut(char) -> bool,
{
    // Default (no buffer case table) path: use Unicode case, matching GNU's
    // standard case table over the ASCII/Unicode range.
    replace_match_case_action_lisp_cased(
        matched,
        is_word_char,
        char::is_uppercase,
        char::is_lowercase,
    )
}

/// Like [`replace_match_case_action_lisp`] but with caller-supplied uppercase /
/// lowercase predicates, so a buffer's case table drives GNU's `UPPERCASEP` /
/// `LOWERCASEP` decisions (`src/search.c` `Freplace_match`).
pub(crate) fn replace_match_case_action_lisp_cased<W, U, L>(
    matched: &LispString,
    mut is_word_char: W,
    mut is_upper: U,
    mut is_lower: L,
) -> ReplaceMatchCaseAction
where
    W: FnMut(char) -> bool,
    U: FnMut(char) -> bool,
    L: FnMut(char) -> bool,
{
    let mut some_multiletter_word = false;
    let mut some_lowercase = false;
    let mut some_uppercase = false;
    let mut some_nonuppercase_initial = false;
    let mut prev_is_word = false;

    let bytes = matched.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        pos += len.max(1);
        let ch = char::from_u32(code);
        if ch.is_some_and(&mut is_lower) {
            some_lowercase = true;
            if prev_is_word {
                some_multiletter_word = true;
            } else {
                some_nonuppercase_initial = true;
            }
        } else if ch.is_some_and(&mut is_upper) {
            some_uppercase = true;
            if prev_is_word {
                some_multiletter_word = true;
            }
        } else if !prev_is_word {
            some_nonuppercase_initial = true;
        }

        prev_is_word = ch.is_some_and(&mut is_word_char);
    }

    if !some_lowercase && some_multiletter_word {
        ReplaceMatchCaseAction::AllCaps
    } else if !some_nonuppercase_initial && some_multiletter_word {
        ReplaceMatchCaseAction::CapInitial
    } else if !some_nonuppercase_initial && some_uppercase {
        ReplaceMatchCaseAction::AllCaps
    } else {
        ReplaceMatchCaseAction::NoChange
    }
}

/// Default "is this a word constituent?" predicate for
/// `apply_replace_match_case`.
///
/// Mirrors GNU's standard syntax table: ASCII letters and digits are
/// `Sword`, `_` is `Ssymbol`, so `_` is not a word constituent in
/// the default baseline. Callers who want to honor
/// `case-symbols-as-words` or per-mode syntax tables should use
/// `apply_replace_match_case_with` with a closure that consults
/// `BVAR(current_buffer, syntax_table)`. See audit findings #14 and
/// #20 in `drafts/regex-search-audit.md`.
fn default_is_word_char(ch: char) -> bool {
    if ch.is_ascii_alphanumeric() {
        return true;
    }
    // GNU standard-syntax-table puts `$` and `%` in Sword. Neomacs's
    // `SyntaxTable::new_standard` agrees. Leave them inline here to
    // keep this hot path allocation-free.
    matches!(ch, '$' | '%')
}

pub(crate) fn builtin_downcase_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::builtins::casing_word_predicate(ctx);
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_region_in_state(ctx, args, "downcase-region", move |s| {
        downcase_lisp_string_emacs_compat(s, is_word, &casetab)
    })
}

pub(crate) fn builtin_upcase_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_region_in_state(ctx, args, "upcase-region", move |s| {
        upcase_lisp_string_emacs_compat(s, &casetab)
    })
}

pub(crate) fn builtin_capitalize_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(ctx);
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_region_in_state(ctx, args, "capitalize-region", move |s| {
        capitalize_lisp_string(s, is_word, &casetab)
    })
}

pub(crate) fn builtin_upcase_initials_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(ctx);
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_region_in_state(ctx, args, "upcase-initials-region", move |s| {
        upcase_initials_lisp_string(s, is_word, &casetab)
    })
}

pub(crate) fn builtin_downcase_word(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::builtins::casing_word_predicate(ctx);
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_word_in_state(ctx, args, "downcase-word", move |s| {
        downcase_lisp_string_emacs_compat(s, is_word, &casetab)
    })
}

pub(crate) fn builtin_upcase_word(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_word_in_state(ctx, args, "upcase-word", move |s| {
        upcase_lisp_string_emacs_compat(s, &casetab)
    })
}

pub(crate) fn builtin_capitalize_word(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = crate::emacs_core::syntax::casing_word_predicate(ctx);
    let casetab = CaseTableOverride::for_current_buffer(ctx)?;
    casify_word_in_state(ctx, args, "capitalize-word", move |s| {
        capitalize_lisp_string(s, is_word, &casetab)
    })
}

/// `(char-resolve-modifiers CHAR)` -- resolve modifier bits in character.
/// Resolve shift/control modifiers into the base character where possible.
pub(crate) fn builtin_char_resolve_modifiers(args: Vec<Value>) -> EvalResult {
    expect_args("char-resolve-modifiers", &args, 1)?;

    let code = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), args[0]],
            ));
        }
    };

    Ok(Value::fixnum(
        crate::emacs_core::emacs_char::char_resolve_modifier_mask(code),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
