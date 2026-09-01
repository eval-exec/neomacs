use super::*;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use malachite::integer::Integer;

fn install_test_runtime() {
    // Tagged heap is auto-created in test mode; no manual setup needed.
}

#[test]
fn file_user_uid_matches_user_uid() {
    crate::test_utils::init_test_tracing();
    let user_uid = builtin_user_uid(vec![]).expect("user-uid should succeed");
    let file_user_uid = builtin_file_user_uid(vec![]).expect("file-user-uid should succeed");
    assert_eq!(file_user_uid, user_uid);
    assert!(file_user_uid.is_fixnum());
}

#[test]
fn file_user_uid_arity_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_file_user_uid(vec![Value::NIL]).is_err());
}

#[test]
fn file_group_gid_matches_group_gid() {
    crate::test_utils::init_test_tracing();
    let group_gid = builtin_group_gid(vec![]).expect("group-gid should succeed");
    let file_group_gid = builtin_file_group_gid(vec![]).expect("file-group-gid should succeed");
    assert_eq!(file_group_gid, group_gid);
    assert!(file_group_gid.is_fixnum());
}

#[test]
fn process_identity_queries_do_not_construct_child_commands() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::callproc::reset_new_child_command_calls_for_test();

    builtin_user_uid(vec![]).expect("user-uid should succeed");
    builtin_user_real_uid(vec![]).expect("user-real-uid should succeed");
    builtin_file_user_uid(vec![]).expect("file-user-uid should succeed");
    builtin_group_gid(vec![]).expect("group-gid should succeed");
    builtin_group_real_gid(vec![]).expect("group-real-gid should succeed");
    builtin_file_group_gid(vec![]).expect("file-group-gid should succeed");

    assert_eq!(
        crate::emacs_core::callproc::new_child_command_calls_for_test(),
        0,
        "credential queries are host syscalls, not subprocess operations"
    );
}

#[test]
fn file_group_gid_arity_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_file_group_gid(vec![Value::NIL]).is_err());
}

// -- expect_args / expect_min_args / expect_max_args ----------------------

#[test]
fn expect_args_exact_match() {
    crate::test_utils::init_test_tracing();
    assert!(expect_args("test", &[Value::NIL, Value::NIL], 2).is_ok());
}

#[test]
fn expect_args_wrong_count() {
    crate::test_utils::init_test_tracing();
    let err = expect_args("test", &[Value::NIL], 2);
    assert!(err.is_err());
}

#[test]
fn expect_min_args_at_min() {
    crate::test_utils::init_test_tracing();
    assert!(expect_min_args("test", &[Value::NIL], 1).is_ok());
}

#[test]
fn expect_min_args_below_min() {
    crate::test_utils::init_test_tracing();
    assert!(expect_min_args("test", &[], 1).is_err());
}

#[test]
fn expect_max_args_at_max() {
    crate::test_utils::init_test_tracing();
    assert!(expect_max_args("test", &[Value::NIL, Value::NIL], 2).is_ok());
}

#[test]
fn expect_max_args_above_max() {
    crate::test_utils::init_test_tracing();
    assert!(expect_max_args("test", &[Value::NIL, Value::NIL, Value::NIL], 2).is_err());
}

// -- expect_integer -------------------------------------------------------

#[test]
fn expect_integer_from_int() {
    crate::test_utils::init_test_tracing();
    assert_eq!(expect_integer("test", &Value::fixnum(42)).unwrap(), 42);
}

#[test]
fn expect_integer_from_non_int() {
    crate::test_utils::init_test_tracing();
    assert!(expect_integer("test", &Value::NIL).is_err());
}

// -- collect_insert_text --------------------------------------------------

#[test]
fn collect_insert_text_strings_and_chars() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let s = Value::string("hello");
    let c = Value::char('!');
    let result = collect_insert_text("insert", &[s, c]).unwrap();
    assert_eq!(result, b"hello!");
}

#[test]
fn collect_insert_text_int_as_char() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // ASCII 65 = 'A'
    let result = collect_insert_text("insert", &[Value::fixnum(65)]).unwrap();
    assert_eq!(result, b"A");
}

#[test]
fn collect_insert_text_nonunicode_char_preserves_emacs_code() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let code = 0x3F_FF80i64;
    let result = collect_insert_text("insert", &[Value::fixnum(code)]).unwrap();
    // Raw byte 0x80 encodes as the overlong C0/C1 two-byte sequence
    // [0xC0, 0x80] in Emacs internal encoding.
    let (decoded, len) = crate::emacs_core::emacs_char::string_char(&result);
    assert_eq!(decoded, code as u32);
    assert_eq!(len, result.len());
}

#[test]
fn collect_insert_text_wrong_type() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    assert!(collect_insert_text("insert", &[Value::NIL]).is_err());
}

// -- builtin_logcount -----------------------------------------------------

#[test]
fn logcount_positive() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // 7 = 0b111 → 3 bits
    let result = builtin_logcount(vec![Value::fixnum(7)]).unwrap();
    assert_eq!(result, Value::fixnum(3));
}

#[test]
fn logcount_zero() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let result = builtin_logcount(vec![Value::fixnum(0)]).unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn logcount_negative() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    // -1 = all 1s → !(-1) = 0 → count_ones = 0
    let result = builtin_logcount(vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    // -2 = ...1110 → !(-2) = 1 → count_ones = 1
    let result = builtin_logcount(vec![Value::fixnum(-2)]).unwrap();
    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn logcount_bignum_matches_gnu_integer_path() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let big = Value::make_integer(Integer::from(1u64) << 100u32);
    assert_eq!(builtin_logcount(vec![big]).unwrap(), Value::fixnum(1));

    let mut negative_big = Integer::from(1);
    negative_big <<= 100;
    negative_big = -negative_big;
    let negative = Value::make_integer(negative_big);
    assert_eq!(
        builtin_logcount(vec![negative]).unwrap(),
        Value::fixnum(100)
    );
}

#[test]
fn logcount_wrong_type() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    assert!(builtin_logcount(vec![Value::NIL]).is_err());
}

#[test]
fn erase_buffer_widens_before_deleting_current_contents() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let obarray = Obarray::new();
    let dynamic: Vec<OrderedRuntimeBindingMap> = Vec::new();
    let mut buffers = crate::buffer::BufferManager::new();
    let current = buffers.current_buffer_id().expect("current buffer");
    let _ = buffers.insert_into_buffer(current, "abcdef");
    {
        let buf = buffers.get_mut(current).expect("buffer");
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 4));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    }

    let result = erase_buffer_impl(&obarray, &dynamic, &mut buffers, vec![]);
    assert!(result.as_ref().map_or(false, |v| v.is_nil()));

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

    let mut eval = crate::emacs_core::Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .insert_into_buffer(current, "abcdef")
        .expect("insert test text");
    eval.treesit.set_linecol_cache(
        current,
        crate::emacs_core::treesit::LineColCache {
            line: 3,
            col: 4,
            bytepos: 5,
        },
    );
    eval.obarray
        .set_symbol_value("combine-after-change-calls", Value::T);

    signal_after_change(&mut eval, EmacsByteRange::from_usize(1, 2), CharLen::ZERO)
        .expect("after-change signal");

    let cache = eval
        .treesit
        .linecol_cache(current)
        .expect("linecol cache should remain installed");
    assert_eq!((cache.line, cache.col, cache.bytepos), (1, 1, 0));
    assert_eq!(eval.combine_after_change_list.len(), 1);
}

#[test]
fn combine_after_change_special_case_uses_gnu_symbol_identity() {
    crate::test_utils::init_test_tracing();
    install_test_runtime();

    let mut eval = crate::emacs_core::Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    let before_sym = crate::emacs_core::intern::intern("before-change-functions");
    eval.obarray
        .set_symbol_value("combine-after-change-calls", Value::T);
    eval.obarray
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
            Value::symbol(crate::emacs_core::intern::intern_uninterned(
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

    let mut eval = crate::emacs_core::Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
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
    let buf = eval.buffers.get(current).expect("current buffer");
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
        let mut eval = crate::emacs_core::Context::new();
        let current = eval.buffers.current_buffer_id().expect("current buffer");
        eval.buffers
            .insert_into_buffer(current, "aaaa")
            .expect("insert test text");
        {
            let buf = eval.buffers.get_mut(current).expect("buffer");
            assert!(buf.get_multibyte(), "default buffer must be multibyte");
            // 1-based lisp char N for all-ASCII text is the 0-based byte N-1.
            buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(point_char - 1));
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

        let buf = eval.buffers.get(current).expect("buffer");
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
        let mut eval = crate::emacs_core::Context::new();
        let current = eval.buffers.current_buffer_id().expect("current buffer");
        eval.buffers
            .insert_into_buffer(current, "aaaa")
            .expect("insert test text");
        {
            let buf = eval.buffers.get_mut(current).expect("buffer");
            buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(point_char - 1));
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

        let buf = eval.buffers.get(current).expect("buffer");
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
        let mut eval = crate::emacs_core::Context::new();
        let current = eval.buffers.current_buffer_id().expect("current buffer");
        eval.buffers
            .insert_into_buffer(current, "abcdabcd")
            .expect("insert test text");
        {
            let buf = eval.buffers.get_mut(current).expect("buffer");
            buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(point_char - 1));
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

        let buf = eval.buffers.get(current).expect("buffer");
        assert_eq!(buf.buffer_string(), "XX");
        assert_eq!(
            buf.point_lisp_char_pos().as_i64(),
            expected_point,
            "point must match GNU for goto-char {point_char}"
        );
    }
}
