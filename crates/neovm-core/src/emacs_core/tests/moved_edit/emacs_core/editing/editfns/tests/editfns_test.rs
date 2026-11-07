use super::*;

use neovm_edit::buffer::overlay::OverlayModificationHook;
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_value::chartab::{builtin_char_table_subtype, ct_lookup_with_symbols, is_char_table};
use neovm_value::emacs_char;
use neovm_value::emacs_char::{
    MAX_CHAR, MAX_MULTIBYTE_LENGTH, byte8_to_char, char_string, chars_in_multibyte,
    string_char_advance,
};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::LispString;
use neovm_value::symbol::{SymbolRedirect, SymbolTrappedWrite};
use neovm_value::value::ValueKind;
use std::ffi::CStr;
use strum::IntoStaticStr;

use malachite::integer::Integer;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::byte_positions::EmacsByteRange;
use neovm_value::value::OrderedRuntimeBindingMap;
use neovm_value::value::Value;
use neovm_value::value::positions::CharLen;

use neovm_edit::emacs_core::editfns::*;

use crate::emacs_core::identity_commands::{
    builtin_group_gid, builtin_group_real_gid, builtin_user_real_uid, builtin_user_uid,
};

use neovm_system::emacs_core::callproc::new_child_command_calls_for_test;
use neovm_system::test_support::authority::reset_new_child_command_calls_for_test;
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};

#[test]
fn inhibited_buffer_changes_preserve_pending_mark_deactivation() {
    let mut ev = crate::test_utils::new_context();
    ev.eval_str("(progn (set-buffer (get-buffer-create \"silent-selection\")) (insert \"abc\"))")
        .unwrap();
    // GNU insdel.c returns before setting deactivate-mark for every kind of
    // inhibited modification, including actual character changes.
    for change in [
        "(put-text-property 1 2 'invisible t)",
        "(insert \"x\")",
        "(delete-region 1 2)",
    ] {
        for pending in ["nil", "t"] {
            let expression = format!(
                "(let ((inhibit-modification-hooks t) (deactivate-mark {pending}))
                   {change} deactivate-mark)"
            );
            let actual = ev.eval_str(&expression).unwrap();
            assert_eq!(
                actual,
                if pending == "nil" {
                    Value::NIL
                } else {
                    Value::T
                },
                "{expression}"
            );
        }
    }
    // The callback-free ordinary path must still request deactivation.
    assert_eq!(
        ev.eval_str(
            "(let ((inhibit-modification-hooks nil) (deactivate-mark nil))
           (put-text-property 1 2 'face 'bold) deactivate-mark)"
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn process_identity_queries_do_not_construct_child_commands() {
    crate::test_utils::init_test_tracing();
    reset_new_child_command_calls_for_test();

    builtin_user_uid(vec![]).expect("user-uid should succeed");
    builtin_user_real_uid(vec![]).expect("user-real-uid should succeed");
    builtin_group_gid(vec![]).expect("group-gid should succeed");
    builtin_group_real_gid(vec![]).expect("group-real-gid should succeed");

    assert_eq!(
        new_child_command_calls_for_test(),
        0,
        "credential queries are host syscalls, not subprocess operations"
    );
}

#[test]
fn erase_buffer_widens_before_deleting_current_contents() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let obarray = neovm_value::symbol::Obarray::new();
    let dynamic: Vec<OrderedRuntimeBindingMap> = Vec::new();
    let mut buffers = neovm_edit::buffer::BufferManager::new();
    let current = buffers.current_buffer_id().expect("current buffer");
    let _ = buffers.insert_into_buffer(current, "abcdef");
    {
        let buf = buffers.get_mut(current).expect("buffer");
        buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(2, 4));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(4));
    }

    let result = erase_buffer_impl(
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
        &dynamic,
        &mut buffers,
        vec![],
    );
    assert!(result.as_ref().is_ok_and(|v| v.is_nil()));

    let buf = buffers.get(current).expect("buffer after erase");
    assert_eq!(buf.buffer_string(), "");
    assert_eq!(buf.accessible_char_len().get(), 0);
    assert_eq!(buf.point_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_min_emacs_byte_pos().get(), 0);
    assert_eq!(buf.point_max_emacs_byte_pos().get(), 0);
}

#[test]
fn combine_after_change_still_updates_treesit_linecol_cache() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let mut eval = crate::test_utils::new_context();
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
        .insert_into_buffer(current, "abcdef")
        .expect("insert test text");
    neovm_eval::test_support::authority::set_linecol_cache_for_test(&mut eval, current, 3, 4, 5);
    eval.obarray_mut()
        .set_symbol_value("combine-after-change-calls", Value::T);

    signal_after_change(&mut eval, EmacsByteRange::from_usize(1, 2), CharLen::ZERO)
        .expect("after-change signal");

    let cache = eval
        .treesit()
        .linecol_cache(current)
        .expect("linecol cache should remain installed");
    assert_eq!((cache.line(), cache.col(), cache.bytepos()), (1, 1, 0));
    assert_eq!(eval.edit.combine_after_change_list().len(), 1);
}

#[test]
fn combine_after_change_special_case_uses_gnu_symbol_identity() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let mut eval = crate::test_utils::new_context();
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let before_sym = neovm_value::intern::intern("before-change-functions");
    eval.obarray_mut()
        .set_symbol_value("combine-after-change-calls", Value::T);
    eval.obarray_mut()
        .set_symbol_value("before-change-functions", Value::NIL);
    eval.set_buffer_local_binding_by_id(
        current,
        before_sym,
        Value::list(vec![Value::T, Value::symbol("syntax-ppss-flush-cache")]),
    )
    .expect("install canonical before-change-functions shape");

    assert_eq!(
        BeforeChangeSpecialFunction::SyntaxPpssFlushCache.name(),
        "syntax-ppss-flush-cache"
    );
    assert!(combine_after_change_calls_active(&eval));

    eval.set_buffer_local_binding_by_id(
        current,
        before_sym,
        Value::list(vec![
            Value::T,
            Value::symbol(neovm_value::intern::intern_uninterned(
                "syntax-ppss-flush-cache",
            )),
        ]),
    )
    .expect("install uninterned same-name before-change function");

    assert!(!combine_after_change_calls_active(&eval));
}

#[test]
fn translate_region_accepts_real_char_table_translation_table() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let mut eval = crate::test_utils::new_context();
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
        .insert_into_buffer(current, "abc")
        .expect("insert test text");

    let table = Value::make_char_table(Value::symbol("translation-table"), Value::NIL, 0);
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![table, Value::fixnum('b' as i64), Value::fixnum('x' as i64)],
        None,
    )
    .expect("set translation table entry");

    let changed = builtin_translate_region_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(4), table],
    )
    .expect("translate region through real char-table");

    assert_eq!(changed, Value::fixnum(1));
    let buf = eval.edit.buffers.get(current).expect("current buffer");
    assert_eq!(buf.buffer_string(), "axc");
}

#[test]
fn translate_region_keeps_point_char_position_when_char_grows() {
    // GNU translates one character at a time via `replace_range`, which moves
    // point only by the *character* delta of replacements ending at or before
    // point.  A char->char translation preserves the character count, so point
    // stays at the same character position even when the byte length grows.
    //
    // Oracle (GNU Emacs):
    //   (let ((tt (make-translation-table '((?a . ?é)))))
    //     (with-temp-buffer (insert "aaaa") (goto-char N)
    //       (translate-region (point-min) (point-max) tt)
    //       (list (point) (buffer-string))))
    //   N=1 => (1 "éééé")  N=2 => (2 "éééé")
    //   N=3 => (3 "éééé")  N=5 => (5 "éééé")
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // `?a` (1 byte) -> `?é` (2 bytes in multibyte text): a growing char->char
    // translation.  Point must keep its character position for every starting
    // point.
    for point_char in [1usize, 2, 3, 5] {
        let mut eval = crate::test_utils::new_context();
        let current = eval
            .edit
            .buffers
            .current_buffer_id()
            .expect("current buffer");
        eval.edit
            .buffers
            .insert_into_buffer(current, "aaaa")
            .expect("insert test text");
        {
            let buf = eval.edit.buffers.get_mut(current).expect("buffer");
            assert!(buf.get_multibyte(), "default buffer must be multibyte");
            // 1-based lisp char N for all-ASCII text is the 0-based byte N-1.
            buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(point_char - 1));
        }

        let table = Value::make_char_table(Value::symbol("translation-table"), Value::NIL, 0);
        crate::emacs_core::chartable::builtin_set_char_table_range(
            vec![table, Value::fixnum('a' as i64), Value::fixnum('é' as i64)],
            None,
        )
        .expect("set translation table entry");

        let changed = builtin_translate_region_internal(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(5), table],
        )
        .expect("translate region");
        assert_eq!(changed, Value::fixnum(4));

        let buf = eval.edit.buffers.get(current).expect("buffer");
        assert_eq!(buf.buffer_string(), "éééé");
        assert_eq!(
            buf.point_lisp_char_pos().as_i64(),
            point_char as i64,
            "point char position must be preserved for goto-char {point_char}"
        );
    }
}

#[test]
fn translate_region_advances_point_by_char_delta_for_growing_vector() {
    // A vector translation `?a -> [?x ?y]` grows each source character into two
    // characters.  GNU advances point by the cumulative character delta of all
    // replacements ending at or before point.
    //
    // Oracle (GNU Emacs):
    //   (let ((tt (make-char-table 'translation-table)))
    //     (aset tt ?a (vector ?x ?y))
    //     (with-temp-buffer (insert "aaaa") (goto-char N)
    //       (translate-region (point-min) (point-max) tt)
    //       (list (point) (buffer-string))))
    //   N=1 => (1 "xyxyxyxy")  N=2 => (3 "xyxyxyxy")
    //   N=3 => (5 "xyxyxyxy")  N=5 => (9 "xyxyxyxy")
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    for (point_char, expected_point) in [(1usize, 1i64), (2, 3), (3, 5), (5, 9)] {
        let mut eval = crate::test_utils::new_context();
        let current = eval
            .edit
            .buffers
            .current_buffer_id()
            .expect("current buffer");
        eval.edit
            .buffers
            .insert_into_buffer(current, "aaaa")
            .expect("insert test text");
        {
            let buf = eval.edit.buffers.get_mut(current).expect("buffer");
            buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(point_char - 1));
        }

        let table = Value::make_char_table(Value::symbol("translation-table"), Value::NIL, 0);
        let replacement =
            Value::make_vector(vec![Value::fixnum('x' as i64), Value::fixnum('y' as i64)]);
        crate::emacs_core::chartable::builtin_set_char_table_range(
            vec![table, Value::fixnum('a' as i64), replacement],
            None,
        )
        .expect("set translation table vector entry");

        let changed = builtin_translate_region_internal(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(5), table],
        )
        .expect("translate region");
        assert_eq!(changed, Value::fixnum(8));

        let buf = eval.edit.buffers.get(current).expect("buffer");
        assert_eq!(buf.buffer_string(), "xyxyxyxy");
        assert_eq!(
            buf.point_lisp_char_pos().as_i64(),
            expected_point,
            "point must advance by char delta for goto-char {point_char}"
        );
    }
}

#[test]
fn translate_region_clamps_point_inside_shrinking_multi_char_source() {
    // Multi-character source translation `([?a ?b ?c ?d] . ?X)` consumes 4
    // characters and produces 1.  When point falls strictly inside a consumed
    // span, GNU's `replace_range` clamps it to the end of the replacement
    // (`adjust_point (from + inschars - PT)`); when the span ends at or before
    // point, point shifts by the net character delta.
    //
    // Oracle (GNU Emacs), buffer "abcdabcd":
    //   (let ((tt (make-char-table 'translation-table)))
    //     (aset tt ?a (list (cons (vector ?a ?b ?c ?d) ?X)))
    //     (with-temp-buffer (insert "abcdabcd") (goto-char N)
    //       (translate-region (point-min) (point-max) tt)
    //       (point)))
    //   N=1 => 1  N=2..5 => 2  N=6..9 => 3   (buffer becomes "XX")
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    for (point_char, expected_point) in [
        (1usize, 1i64),
        (2, 2),
        (3, 2),
        (4, 2),
        (5, 2),
        (6, 3),
        (7, 3),
        (8, 3),
        (9, 3),
    ] {
        let mut eval = crate::test_utils::new_context();
        let current = eval
            .edit
            .buffers
            .current_buffer_id()
            .expect("current buffer");
        eval.edit
            .buffers
            .insert_into_buffer(current, "abcdabcd")
            .expect("insert test text");
        {
            let buf = eval.edit.buffers.get_mut(current).expect("buffer");
            buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(point_char - 1));
        }

        let table = Value::make_char_table(Value::symbol("translation-table"), Value::NIL, 0);
        let from_vector = Value::make_vector(vec![
            Value::fixnum('a' as i64),
            Value::fixnum('b' as i64),
            Value::fixnum('c' as i64),
            Value::fixnum('d' as i64),
        ]);
        let entry = Value::list(vec![Value::cons(from_vector, Value::fixnum('X' as i64))]);
        crate::emacs_core::chartable::builtin_set_char_table_range(
            vec![table, Value::fixnum('a' as i64), entry],
            None,
        )
        .expect("set translation table multi-char entry");

        let changed = builtin_translate_region_internal(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(9), table],
        )
        .expect("translate region");
        assert_eq!(changed, Value::fixnum(2));

        let buf = eval.edit.buffers.get(current).expect("buffer");
        assert_eq!(buf.buffer_string(), "XX");
        assert_eq!(
            buf.point_lisp_char_pos().as_i64(),
            expected_point,
            "point must match GNU for goto-char {point_char}"
        );
    }
}

/// `buffer-substring-no-properties` now takes its character count from the
/// positions it already resolved instead of re-walking the copied bytes, so
/// the count has to be right for every shape of multibyte text: pure ASCII,
/// multi-byte characters, a region that starts and ends mid-buffer, reversed
/// arguments, an empty region, and a narrowed buffer.
#[test]
fn buffer_substring_no_properties_reports_the_right_length_for_multibyte_text() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    // Mixed widths: ASCII, 2-byte, 3-byte and 4-byte characters.
    ev.eval_str(
        "(progn (set-buffer (get-buffer-create \"substr-probe\"))
                (insert \"abcé日本語x😀z\"))",
    )
    .expect("fill the probe buffer");

    for expr in [
        "(buffer-substring-no-properties (point-min) (point-max))",
        "(buffer-substring-no-properties 4 8)",
        "(buffer-substring-no-properties 8 4)",
        "(buffer-substring-no-properties 3 3)",
        "(buffer-substring-no-properties (point-min) 5)",
    ] {
        let got = neovm_eval::emacs_core::print::format_eval_result(&ev.eval_str(expr));
        // `length` walks the string's own character count, so it disagrees
        // with the bytes exactly when the cached count is wrong.
        let len = neovm_eval::emacs_core::print::format_eval_result(
            &ev.eval_str(&format!("(length {expr})")),
        );
        let via_chars = neovm_eval::emacs_core::print::format_eval_result(
            &ev.eval_str(&format!("(length (append {expr} nil))")),
        );
        assert_eq!(
            len, via_chars,
            "{expr}: stored length disagrees with the characters ({got})"
        );
    }

    // Narrowing moves point-min/point-max; the count must follow.
    let narrowed = neovm_eval::emacs_core::print::format_eval_result(&ev.eval_str(
        "(progn (narrow-to-region 4 9)
                (let ((s (buffer-substring-no-properties (point-min) (point-max))))
                  (prog1 (list (length s) (length (append s nil))) (widen))))",
    ));
    let parts: Vec<&str> = narrowed
        .trim_start_matches("OK (")
        .trim_end_matches(')')
        .split(' ')
        .collect();
    assert_eq!(parts.len(), 2, "narrowed probe returned {narrowed}");
    assert_eq!(
        parts[0], parts[1],
        "narrowed: stored length disagrees ({narrowed})"
    );
}

fn install_test_runtime() {
    // Tagged heap is auto-created in test mode; no manual setup needed.
}
