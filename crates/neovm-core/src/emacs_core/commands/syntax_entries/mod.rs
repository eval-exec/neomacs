//! Syntax Lisp entries that stay in the core command layer (stage-3b R0).
//!
//! `forward-comment`, `backward-prefix-chars` and `forward-word` bind
//! evaluator state (Lisp callbacks, match data, syntax-propertize), so under
//! R0 they stay core-side.  The syntax engine (scanners, caches, snapshots)
//! stays in the text layer and is called downward through the
//! TextHost-generic helpers.  The registered subrs are unchanged: the
//! world_subr! declarations stay in `subrs/mod.rs`.

use crate::buffer::CharPos0;
use crate::emacs_core::eval::VariableView;
use crate::emacs_core::syntax::{
    SexpScanPolicy, SyntaxProperties, SyntaxTable, buffer_byte_to_lisp_pos,
    buffer_char_to_emacs_byte_pos, builtin_backward_prefix_chars_in_buffers,
    expect_forward_comment_count, forward_comment_in_buffers, forward_comment_scan_in_buffer,
    forward_word_with_options, maybe_syntax_propertize_for_scan,
    parse_sexp_lookup_properties_enabled, syntax_propertize_frontier_for_scan,
    word_boundary_table_active, word_motion_with_table,
};
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_value::error::{EvalResult, LispCondition, signal};
use neovm_value::value::{Value, ValueKind};

/// `(forward-comment COUNT)` — move point over COUNT comment/whitespace
/// constructs. Returns `t` if all COUNT were successfully skipped, `nil`
/// if scanning stopped early (hit non-comment/non-whitespace or buffer
/// boundary).
pub(crate) fn builtin_forward_comment(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let delegated = eval
        .eval_symbol("forward-comment-function")
        .unwrap_or(Value::NIL);
    if !delegated.is_nil() {
        return eval.apply(delegated, args);
    }

    let count = expect_forward_comment_count(&args)?;
    let honor = parse_sexp_lookup_properties_enabled(eval);

    if honor && count > 0 {
        // GNU propertizes lazily, only as far as the scan actually reaches.
        // Propertizing the whole accessible tail here made every post-edit
        // `forward-comment` re-run syntax-propertize from the edit point to
        // point-max, which is quadratic under newcomment's per-line loops.
        // Instead propertize a bounded window; the scan below is side-effect
        // free (point commits only at the end) and moves strictly forward, so
        // if it stopped near the propertized frontier we widen and re-run.
        let (current_id, point_char, end_char) = {
            let buf = eval
                .edit
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            (
                buf.id(),
                buf.point_char_pos().get(),
                buf.accessible_char_region().end().get(),
            )
        };
        // GNU `parse_sexp_propertize (charpos)` asks Lisp for
        // `min (zv, charpos + 1)` and lets `syntax-propertize` decide how far
        // it actually goes (its own `syntax-propertize-chunk-size`, 500);
        // the scan then runs up to that frontier (`gl_state.e_property`)
        // and asks again only when it crosses it.  A fixed 1024-char window
        // here re-propertized 2x GNU's text after EVERY flush -- newcomment's
        // per-line insert flushes `syntax-propertize--done` back to the
        // edit, so a comment-region over 100 lines paid ~176 Lisp calls of
        // ~1K chars each (41% of the window; GNU's whole window was smaller).
        let mut target = point_char.saturating_add(2);
        let mut last_window_end = 0usize;
        loop {
            maybe_syntax_propertize_for_scan(eval, target)?;
            let mut window_end = syntax_propertize_frontier_for_scan(eval, point_char, end_char);
            if window_end <= last_window_end {
                // The frontier did not advance (GNU: "internal--syntax-propertize
                // did not move syntax-propertize--done"); scan the rest as is
                // rather than spin.
                window_end = end_char;
            }
            last_window_end = window_end;
            // This is one semantic snapshot boundary: propertization above
            // ran arbitrary Lisp and may have changed either the property
            // resolver inputs or buffer-local comment escape policy.
            let policy = SexpScanPolicy::for_context(eval);
            let props = SyntaxProperties::for_scan_with_symbols(
                honor,
                eval.variables(),
                &eval.edit.buffers,
                eval.symbol_mode(),
            );
            let (ok, final_pos, final_char) = {
                let buf = eval
                    .edit
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let (ok, final_pos) =
                    forward_comment_scan_in_buffer(buf, count, props, policy.comment_end_escape());
                let final_char = buf.emacs_byte_pos_to_char_pos_clamped(final_pos).get();
                (ok, final_pos, final_char)
            };
            // Comment start/end matching peeks at most one char past the
            // cursor, so a stop 2+ chars inside the window read only
            // propertized text.
            if window_end >= end_char || final_char.saturating_add(2) <= window_end {
                let _ = eval
                    .edit
                    .buffers
                    .goto_buffer_emacs_byte_pos(current_id, final_pos);
                return Ok(if ok { Value::T } else { Value::NIL });
            }
            // Crossed the frontier: GNU's `UPDATE_SYNTAX_TABLE_FORWARD`
            // would call `parse_sexp_propertize (window_end)` here.
            target = window_end.saturating_add(2);
        }
    }

    if honor {
        // count <= 0: the scan only reads at or before point.
        let target = eval
            .edit
            .buffers
            .current_buffer()
            .map(|buf| buf.point_char_pos().get())
            .unwrap_or(0);
        if target > 0 {
            maybe_syntax_propertize_for_scan(eval, target)?;
        }
    }

    // As above, snapshot all Lisp-visible scan controls only after the last
    // possible syntax-propertize callback.
    let policy = SexpScanPolicy::for_context(eval);
    let symbol_mode = eval.symbol_mode();
    let (obarray, owner, buffers) = eval.variable_view_and_buffers_mut();
    let props = SyntaxProperties::for_scan_with_symbols(
        honor,
        VariableView::new(obarray, owner),
        buffers,
        symbol_mode,
    );
    forward_comment_in_buffers(buffers, count, props, policy.comment_end_escape())
}

/// `(backward-prefix-chars)` — move point backward over prefix-syntax chars.
pub(crate) fn builtin_backward_prefix_chars(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    let honor = parse_sexp_lookup_properties_enabled(eval);
    let symbol_mode = eval.symbol_mode();
    let (obarray, owner, buffers) = eval.variable_view_and_buffers_mut();
    let props = SyntaxProperties::for_scan_with_symbols(
        honor,
        VariableView::new(obarray, owner),
        buffers,
        symbol_mode,
    );
    builtin_backward_prefix_chars_in_buffers(buffers, args, props)
}

/// GNU `find-word-boundary-function-table`'s symbol (DEFVAR_LISP,
/// src/syntax.c:3793), interned once.
fn find_word_boundary_function_table_sym() -> neovm_value::intern::SymId {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| neovm_value::intern::intern("find-word-boundary-function-table"))
}

/// `(forward-word &optional COUNT)` — move point forward COUNT words.
pub(crate) fn builtin_forward_word(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(eval);
    let orig_byte = {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        buf.point_emacs_byte_pos()
    };
    // When `find-word-boundary-function-table` is active (subword/superword),
    // GNU's scan_words consults it per word; otherwise the plain syntax scan is
    // used unchanged. GNU reads Vfind_word_boundary_function_table
    // (DEFVAR_LISP, src/syntax.c:3793) through do_symval_forwarding, so a
    // buffer-local or `let` binding must win over the default: read the
    // visible value, never the buffer-blind default.
    let wbtable = eval
        .builtin_var_value(find_word_boundary_function_table_sym())
        .unwrap_or(Value::NIL);
    let (raw_byte, completed) = if word_boundary_table_active(&wbtable, eval.symbol_mode()) {
        word_motion_with_table(eval, count, honor, wbtable)
    } else {
        let props = SyntaxProperties::for_scan_with_symbols(
            honor,
            eval.variables(),
            &eval.edit.buffers,
            eval.symbol_mode(),
        );
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        let table = SyntaxTable::for_buffer(buf);
        forward_word_with_options(
            buf,
            &table,
            count,
            props,
            super::builtins::search::current_word_boundary_lookup(eval),
        )
    };
    let (orig_char, raw_char) = {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        (
            buffer_byte_to_lisp_pos(buf, orig_byte),
            buffer_byte_to_lisp_pos(buf, raw_byte),
        )
    };

    // GNU `Fforward_word` (syntax.c:1561) constrains the destination via
    // `Fconstrain_to_field` so that motion does not cross input-field
    // boundaries (e.g. the minibuffer prompt). The full call is
    // (constrain-to-field VAL PT nil nil nil).
    let constrained = neovm_edit::emacs_core::buffer::builtin_constrain_to_field_5(
        eval,
        &[
            Value::fixnum(raw_char),
            Value::fixnum(orig_char),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )?;
    let constrained_char = match constrained.kind() {
        ValueKind::Fixnum(n) => n,
        _ => raw_char,
    };

    let final_byte = if constrained_char == raw_char {
        raw_byte
    } else {
        let buf = eval.edit.buffers.current_buffer().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        // Convert constrained 1-based char position back to a byte offset.
        let zero_based = (constrained_char - 1).max(0) as usize;
        buffer_char_to_emacs_byte_pos(buf, CharPos0::new(zero_based))
    };

    let current_id = eval.edit.buffers.current_buffer_id().ok_or_else(|| {
        signal(
            LispCondition::Error,
            vec![Value::string("No current buffer")],
        )
    })?;
    let _ = eval
        .edit
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, final_byte);

    // GNU returns t when the requested motion fully succeeded, nil when it
    // stopped early at a buffer edge or a field boundary.
    Ok(if completed && constrained_char == raw_char {
        Value::T
    } else {
        Value::NIL
    })
}
