use super::*;
use std::fmt;

use neovm_edit::buffer::{BufferManager, EmacsByteRange, LispCharPos1};
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::heap_types::LispString;
use neovm_value::intern::WellKnown;
use neovm_value::tagged::value::SymbolMode;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::{HashMap, HashSet};
use std::ptr;
use strum::{EnumString, IntoStaticStr};

use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::coding::*;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use neovm_value::value::list_to_vec;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::intern::{intern, resolve_sym};

#[test]
fn coding_defvar_lisp_variables_are_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    assert_eq!(
        neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(
            "(list (special-variable-p 'coding-system-for-read)
                   (let ((coding-system-for-read 'chinese-big5-unix))
                     (symbol-value 'coding-system-for-read))
                   (special-variable-p 'coding-system-for-write)
                   (special-variable-p 'last-coding-system-used))"
        )),
        "OK (t chinese-big5-unix t t)"
    );
}

#[test]
fn coding_system_plist_keyword_keys_work_with_builtin_plist_get() {
    crate::test_utils::init_test_tracing();
    let m = mgr();
    let plist = builtin_coding_system_plist(
        &m,
        vec![Value::symbol("utf-8")],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();

    let name = crate::emacs_core::builtins::builtin_plist_get(vec![plist, Value::keyword(":name")])
        .unwrap();
    assert_eq!(name, Value::symbol("utf-8"));

    let mnemonic =
        crate::emacs_core::builtins::builtin_plist_get(vec![plist, Value::keyword(":mnemonic")])
            .unwrap();
    assert_eq!(mnemonic, Value::fixnum('U' as i64));
}

#[test]
fn find_coding_systems_region_internal_accepts_raw_unibyte_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let raw = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        0xFF,
    ]));
    let result = builtin_find_coding_systems_region_internal(&mut eval, vec![raw, Value::NIL]);
    assert_eq!(result.unwrap(), Value::T);
}

#[test]
fn find_coding_systems_region_internal_ignores_bad_exclude_for_ascii() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::string("abc"), Value::NIL, Value::symbol("utf-8")],
    );
    assert_eq!(result.unwrap(), Value::T);
}

#[test]
fn find_coding_systems_region_internal_rejects_non_list_exclude_for_non_ascii_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::string("汉"), Value::NIL, Value::symbol("utf-8")],
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("listp"), Value::symbol("utf-8")]
            );
        }
        other => panic!("expected wrong-type-argument listp, got {other:?}"),
    }
}

#[test]
fn find_coding_systems_region_internal_rejects_non_list_exclude_for_non_ascii_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(insert \"汉\")").expect("insert text");
    let result = builtin_find_coding_systems_region_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(2), Value::symbol("utf-8")],
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("listp"), Value::symbol("utf-8")]
            );
        }
        other => panic!("expected wrong-type-argument listp, got {other:?}"),
    }
}

#[test]
fn detect_coding_region_highest() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.edit
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("abc");
    let result = builtin_detect_coding_region(
        eval.coding_systems(),
        &eval.edit.buffers,
        vec![Value::fixnum(1), Value::fixnum(4), Value::T],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    assert!(result.is_bare_symbol_named("undecided"));
}

#[test]
fn detect_coding_region_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.edit
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("abc");
    let result = builtin_detect_coding_region(
        eval.coding_systems(),
        &eval.edit.buffers,
        vec![Value::fixnum(1), Value::fixnum(4)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_bare_symbol_named("undecided"));
}

#[test]
fn detect_coding_region_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let eval = crate::test_utils::new_context();
    let result = builtin_detect_coding_region(
        eval.coding_systems(),
        &eval.edit.buffers,
        vec![Value::fixnum(1), Value::fixnum(100), Value::NIL, Value::NIL],
        neovm_value::tagged::value::SymbolMode::Bare,
    );
    assert!(result.is_err());
}

#[test]
fn detect_coding_region_rejects_non_integer_or_marker_bounds() {
    crate::test_utils::init_test_tracing();
    let eval = crate::test_utils::new_context();
    assert!(
        builtin_detect_coding_region(
            eval.coding_systems(),
            &eval.edit.buffers,
            vec![Value::string("a"), Value::fixnum(1)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            eval.coding_systems(),
            &eval.edit.buffers,
            vec![Value::fixnum(1), Value::string("b")],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            eval.coding_systems(),
            &eval.edit.buffers,
            vec![Value::NIL, Value::fixnum(1)],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
    assert!(
        builtin_detect_coding_region(
            eval.coding_systems(),
            &eval.edit.buffers,
            vec![Value::fixnum(1), Value::NIL],
            neovm_value::tagged::value::SymbolMode::Bare
        )
        .is_err()
    );
}

#[test]
fn detect_coding_region_validates_accessible_region_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.edit
        .buffers
        .current_buffer_mut()
        .unwrap()
        .insert("abc");

    let err = builtin_detect_coding_region(
        eval.coding_systems(),
        &eval.edit.buffers,
        vec![Value::fixnum(0), Value::fixnum(2)],
        neovm_value::tagged::value::SymbolMode::Bare,
    )
    .expect_err("GNU validate_region rejects positions before point-min");
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            let items = sig.data;
            assert_eq!(items.len(), 3);
            assert!(items[0].is_buffer());
            assert_eq!(items[1].as_fixnum(), Some(0));
            assert_eq!(items[2].as_fixnum(), Some(2));
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn check_coding_systems_region_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(1),
                Value::list(vec![Value::symbol("utf-8")])
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![Value::string("x"), Value::fixnum(1), Value::symbol("utf-8")]
        )
        .unwrap()
        .is_nil()
    );

    let start_type_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![Value::symbol("x"), Value::fixnum(1), Value::symbol("utf-8")],
    )
    .unwrap_err();
    match start_type_err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::symbol("x")]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let type_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![Value::fixnum(1), Value::string("x"), Value::symbol("utf-8")],
    )
    .unwrap_err();
    match type_err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("x")]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    assert!(builtin_check_coding_systems_region(&mut eval, vec![]).is_err());
    assert!(
        builtin_check_coding_systems_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)])
            .is_err()
    );
}

#[test]
fn check_coding_systems_region_matches_gnu_validation_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(insert \"abc\")").expect("insert ascii");

    let range_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::fixnum(9),
            Value::fixnum(10),
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU validates bad buffer ranges before coding systems");
    match range_err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(9), Value::fixnum(10)]);
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }

    let reversed_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(2),
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU does not swap check-coding-systems-region endpoints");
    match reversed_err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(3), Value::fixnum(2)]);
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }

    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(2),
                Value::list(vec![Value::symbol("no-such-coding")]),
            ],
        )
        .expect("GNU ignores coding list for ASCII buffer text")
        .is_nil()
    );

    let coding_err = builtin_check_coding_systems_region(
        &mut eval,
        vec![
            Value::string("é"),
            Value::NIL,
            Value::list(vec![Value::symbol("no-such-coding")]),
        ],
    )
    .expect_err("GNU validates coding list for non-ASCII string text");
    match coding_err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "coding-system-error");
            assert_eq!(sig.data, vec![Value::symbol("no-such-coding")]);
        }
        other => panic!("expected coding-system-error, got {other:?}"),
    }

    assert!(
        builtin_check_coding_systems_region(
            &mut eval,
            vec![
                Value::string("é"),
                Value::NIL,
                Value::cons(Value::symbol("utf-8"), Value::symbol("ignored-tail")),
            ],
        )
        .expect("GNU ignores dotted tail after valid coding cons cars")
        .is_nil()
    );
}

#[test]
fn text_quoting_and_conversion_style_basics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    assert_eq!(
        builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style"),
        Value::symbol("curve")
    );
    for style in ["grave", "straight", "curve"] {
        eval.obarray_mut()
            .set_symbol_value("text-quoting-style", Value::symbol(style));
        assert_eq!(
            builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style explicit style"),
            Value::symbol(style)
        );
    }
    for style in ["nil", "foo", "Curve"] {
        eval.obarray_mut()
            .set_symbol_value("text-quoting-style", Value::symbol(style));
        assert_eq!(
            builtin_text_quoting_style(&eval, vec![]).expect("text-quoting-style fallback"),
            Value::symbol("curve")
        );
    }
    assert!(builtin_text_quoting_style(&eval, vec![Value::NIL]).is_err());
    assert_eq!(
        builtin_set_text_conversion_style(vec![Value::symbol("latin-1")])
            .expect("set-text-conversion-style"),
        Value::NIL
    );
    assert_eq!(
        builtin_set_text_conversion_style(vec![Value::symbol("foo"), Value::symbol("bar")])
            .expect("set-text-conversion-style 2 args"),
        Value::NIL
    );
    assert!(builtin_set_text_conversion_style(vec![]).is_err());
}

/// The nil-`text-quoting-style` decision, branch for branch against GNU
/// `default_to_grave_quoting_style` (`src/doc.c:638-650`).
#[test]
fn text_quoting_style_nil_reads_flag_and_standard_display_table() {
    use neovm_text::emacs_core::chartable::{ct_set_single, make_char_table_with_extra_slots};

    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let style =
        |eval: &Context| builtin_text_quoting_style(eval, vec![]).expect("text-quoting-style");

    // Library contexts are batch-shaped and assume the parity harness's UTF-8
    // locale; a bare context has no display table, so nil is `curve`
    // (src/doc.c:644-645: not a display table -> false).
    assert_eq!(style(&eval), Value::symbol("curve"));

    // `if (!text_quoting_flag) return true;` (src/doc.c:642-643): grave, and
    // the display table is never consulted, whatever it holds.
    eval.obarray_mut()
        .set_symbol_value("internal--text-quoting-flag", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("standard-display-table", Value::T);
    assert_eq!(style(&eval), Value::symbol("grave"));

    // Flag set again: a display table whose U+2018 entry is exactly [?`]
    // resolves to grave (src/doc.c:646-649).
    eval.obarray_mut()
        .set_symbol_value("internal--text-quoting-flag", Value::T);
    let display_table = |entry: Value| {
        let table =
            make_char_table_with_extra_slots(Value::symbol("display-table"), Value::NIL, 18);
        ct_set_single(&table, 0x2018, entry);
        table
    };
    eval.obarray_mut().set_symbol_value(
        "standard-display-table",
        display_table(Value::vector(vec![Value::fixnum(b'`' as i64)])),
    );
    assert_eq!(style(&eval), Value::symbol("grave"));

    // Any other substitution is not the literal backtick doc.c looks for:
    // another character, two glyphs, or the homoglyph-faced glyph code
    // `startup--setup-quote-display` installs (lisp/startup.el:1080-1086).
    for entry in [
        Value::vector(vec![Value::fixnum(b'<' as i64)]),
        Value::vector(vec![Value::fixnum(b'`' as i64), Value::fixnum(b'`' as i64)]),
        Value::vector(vec![Value::fixnum(i64::from(b'`') | (22_i64 << 22))]),
        Value::string("`"),
    ] {
        eval.obarray_mut()
            .set_symbol_value("standard-display-table", display_table(entry));
        assert_eq!(style(&eval), Value::symbol("curve"), "entry {entry:?}");
    }

    // DISP_TABLE_P also checks the purpose and the extra-slot count
    // (src/disptab.h:26-29); a char-table that fails either is no display
    // table and cannot trigger grave.
    let wrong_purpose =
        make_char_table_with_extra_slots(Value::symbol("foo-table"), Value::NIL, 18);
    ct_set_single(
        &wrong_purpose,
        0x2018,
        Value::vector(vec![Value::fixnum(b'`' as i64)]),
    );
    eval.obarray_mut()
        .set_symbol_value("standard-display-table", wrong_purpose);
    assert_eq!(style(&eval), Value::symbol("curve"));

    let wrong_extras =
        make_char_table_with_extra_slots(Value::symbol("display-table"), Value::NIL, 0);
    ct_set_single(
        &wrong_extras,
        0x2018,
        Value::vector(vec![Value::fixnum(b'`' as i64)]),
    );
    eval.obarray_mut()
        .set_symbol_value("standard-display-table", wrong_extras);
    assert_eq!(style(&eval), Value::symbol("curve"));

    // A display table with no own entry inherits its parent's through
    // `char_table_ref` (src/xdisp.c:18425-18432).
    let parent = display_table(Value::vector(vec![Value::fixnum(b'`' as i64)]));
    let child = make_char_table_with_extra_slots(Value::symbol("display-table"), Value::NIL, 18);
    let _ = child.with_char_table_mut(|obj| obj.set_parent(parent));
    eval.obarray_mut()
        .set_symbol_value("standard-display-table", child);
    assert_eq!(style(&eval), Value::symbol("grave"));

    // The nil case of the variable is what consults these inputs; explicit
    // values never do (src/doc.c:666-671).
    eval.obarray_mut()
        .set_symbol_value("text-quoting-style", Value::symbol("straight"));
    assert_eq!(style(&eval), Value::symbol("straight"));
}

#[test]
fn text_quoting_style_variable_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let eval = crate::test_utils::new_context();
    assert_eq!(
        eval.obarray().symbol_value_copied("text-quoting-style"),
        Some(Value::NIL)
    );
}

#[test]
fn find_operation_coding_system_validates_operation_target_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    assert_eq!(
        eval.eval_str(r#"(find-operation-coding-system 'insert-file-contents "x")"#)
            .expect("valid file target returns nil when file-coding-system-alist is nil"),
        Value::NIL
    );

    let invalid = eval
        .eval_str(r#"(find-operation-coding-system 'insert-file-contents (list "x"))"#)
        .expect_err("GNU rejects non-string insert-file-contents targets");
    match invalid {
        neovm_value::error::EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            assert_eq!(
                data,
                vec![Value::string(
                    "Invalid argument 1 of operation ‘insert-file-contents’"
                )]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    let invalid_operation = eval
        .eval_str(r#"(find-operation-coding-system 'not-an-operation "x")"#)
        .expect_err("GNU rejects operations without target-idx");
    match invalid_operation {
        neovm_value::error::EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            assert_eq!(data, vec![Value::string("Invalid first argument")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn detect_iso7_designation_default_priority_is_iso_2022_7bit() {
    crate::test_utils::init_test_tracing();
    register_jisx0208_for_detect();
    // (detect-coding-string (string-to-unibyte "\033$B$3\033(B") t) => iso-2022-7bit
    assert_eq!(detect_highest_with_front(None), "iso-2022-7bit");
    // The list form: (iso-2022-7bit iso-2022-7bit-lock iso-2022-8bit-ss2 iso-2022-jp)
    assert_eq!(
        detect_list(ISO7_DESIGNATION),
        vec![
            "iso-2022-7bit",
            "iso-2022-7bit-lock",
            "iso-2022-8bit-ss2",
            "iso-2022-jp",
        ]
    );
}

#[test]
fn detect_iso7_designation_respects_priority_override() {
    crate::test_utils::init_test_tracing();
    register_jisx0208_for_detect();
    // (with-coding-priority '(iso-2022-jp) ...) => iso-2022-jp (iso-7-tight).
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso7Tight)),
        "iso-2022-jp"
    );
    // (with-coding-priority '(iso-2022-7bit-lock) ...) => iso-2022-7bit-lock.
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso7Else)),
        "iso-2022-7bit-lock"
    );
    // (with-coding-priority '(iso-2022-8bit-ss2) ...) => iso-2022-8bit-ss2.
    assert_eq!(
        detect_highest_with_front(Some(CodingCat::Iso8Else)),
        "iso-2022-8bit-ss2"
    );
}

#[test]
fn check_coding_systems_region_reports_unencodable_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // (us-ascii 2): "é" at buffer position 2 is not us-ascii-encodable.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b)",
            "(check-coding-systems-region (point-min) (point-max) '(us-ascii))",
        ),
        "OK ((us-ascii 2))"
    );
}

#[test]
fn check_coding_systems_region_accepts_string_first_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // String START => 0-based char indices, END ignored: "é" is index 1.
    assert_eq!(
        fmt(
            &mut eval,
            "(check-coding-systems-region (string ?a ?\u{e9} ?b) nil '(us-ascii))",
        ),
        "OK ((us-ascii 1))"
    );
}

#[test]
fn check_coding_systems_region_utf8_encodes_everything() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // utf-8 (charset `unicode`) encodes every character, so it never appears.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b ?\u{3042})",
            "(check-coding-systems-region (point-min) (point-max) '(utf-8 us-ascii))",
        ),
        "OK ((us-ascii 2 4))"
    );
}

#[test]
fn check_coding_systems_region_preserves_input_order_and_charset_membership() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // latin-1 encodes "é" but not "あ" (pos 2); us-ascii encodes neither (1 2).
    // Output keeps the input list order.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9} ?\u{3042})",
            "(check-coding-systems-region (point-min) (point-max) '(latin-1 us-ascii))",
        ),
        "OK ((latin-1 2) (us-ascii 1 2))"
    );
    // latin-1 encodes both "é" and "ü", so the region is fully encodable => nil.
    // (euc-jp/shift_jis are only registered by elisp at runtime, not by the bare
    // `CodingSystemManager::new()`, so they are verified on the release binary.)
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9} ?\u{fc})",
            "(check-coding-systems-region (point-min) (point-max) '(latin-1))",
        ),
        "OK nil"
    );
}

#[test]
fn unencodable_char_position_returns_first_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // No COUNT => the first unencodable position (1-based) as a bare integer.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9})",
            "(unencodable-char-position (point-min) (point-max) 'us-ascii)",
        ),
        "OK 2"
    );
}

#[test]
fn unencodable_char_position_with_count_returns_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // COUNT non-nil => a list of up to COUNT positions in ascending order.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b ?\u{fc})",
            "(unencodable-char-position (point-min) (point-max) 'us-ascii 5)",
        ),
        "OK (2 4)"
    );
}

#[test]
fn unencodable_char_position_string_arg_uses_zero_based_indices() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // STRING arg: START/END index the string; positions are 0-based char indices.
    assert_eq!(
        fmt(
            &mut eval,
            "(unencodable-char-position 0 3 'us-ascii nil (string ?a ?\u{e9} ?b))",
        ),
        "OK 1"
    );
    assert_eq!(
        fmt(
            &mut eval,
            "(unencodable-char-position 0 5 'us-ascii 5 (string ?a ?\u{e9} ?b ?\u{fc} ?c))",
        ),
        "OK (1 3)"
    );
}

#[test]
fn unencodable_char_position_raw_text_and_encodable_return_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // raw-text encodes every byte => nil.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{e9})",
            "(unencodable-char-position (point-min) (point-max) 'raw-text)",
        ),
        "OK nil"
    );
    // utf-8 encodes everything => nil.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?\u{3042})",
            "(unencodable-char-position (point-min) (point-max) 'utf-8)",
        ),
        "OK nil"
    );
}

#[test]
fn unencodable_char_position_swaps_reversed_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // GNU validate_region swaps START/END; check-coding-systems-region does not.
    assert_eq!(
        fmt_buf(
            &mut eval,
            "(string ?a ?\u{e9} ?b)",
            "(unencodable-char-position 4 1 'us-ascii)",
        ),
        "OK 2"
    );
}

/// DIVERGENCES.md entry 143: `Context::eol_conversion` reads the variable the
/// way GNU's C code sees a `DEFVAR_BOOL` -- the dynamic value, which no lexical
/// binding of the name can shadow, and nil when unbound (src/coding.c:12027).
#[test]
fn context_eol_conversion_reads_the_defvar_bool_dynamically() {
    crate::test_utils::init_test_tracing();
    use neovm_text::emacs_core::coding::EolConversion;

    let mut eval = crate::test_utils::new_context();
    assert_eq!(eval.eol_conversion(), EolConversion::Enabled);
    let _ = eval.eval_str("(setq inhibit-eol-conversion t)");
    assert_eq!(eval.eol_conversion(), EolConversion::Inhibited);
    let _ = eval.eval_str("(setq inhibit-eol-conversion nil)");
    assert_eq!(eval.eol_conversion(), EolConversion::Enabled);
    // A non-nil, non-`t` value inhibits too: GNU tests the C bool.
    let _ = eval.eval_str("(setq inhibit-eol-conversion 'anything)");
    assert_eq!(eval.eol_conversion(), EolConversion::Inhibited);
}

/// `:ascii-compatible-p` of a `define-coding-system-internal` system is
/// computed from its charsets, so defining that charset AFTER the plist was
/// memoized must change the next answer: the charset writers drop the memo.
#[test]
fn defining_a_charset_drops_the_coding_system_memo() {
    let mut ev = crate::test_utils::new_context();
    let read = |ev: &mut crate::emacs_core::eval::Context| {
        ev.eval_str("(plist-get (coding-system-plist 'memo-test-cs) :ascii-compatible-p)")
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("plist")
    };
    ev.eval_str(
        "(define-coding-system-internal 'memo-test-cs ?x 'charset '(memo-test-charset)
            nil nil nil nil nil nil nil '(:name memo-test-cs :docstring \"\") nil)",
    )
    .expect("define coding system");
    assert_eq!(
        read(&mut ev),
        "nil",
        "an unknown charset is not ASCII-compatible"
    );
    ev.eval_str(
        "(define-charset-internal 'memo-test-charset 1 [33 126 0 0 0 0 0 0]
            nil nil nil nil nil t nil nil 0 nil nil nil nil nil)",
    )
    .expect("define charset");
    assert_eq!(
        read(&mut ev),
        "t",
        "the memo was dropped by the charset definition"
    );
}

fn mgr() -> CodingSystemManager {
    CodingSystemManager::new()
}

/// Build (priorities, cat_system) matching GNU's runtime state (UTF-8/English
/// language environment, the default).
fn gnu_detect_state() -> CodingDetectionBindings {
    // (category enum index, bound coding-system base name), in GNU priority order.
    let order: [(CodingCat, &str); 20] = [
        (CodingCat::Utf8Nosig, "utf-8"),
        (CodingCat::Iso7, "iso-2022-7bit"),
        (CodingCat::Charset, "iso-latin-1"),
        (CodingCat::Iso7Else, "iso-2022-7bit-lock"),
        (CodingCat::Iso8Else, "iso-2022-8bit-ss2"),
        (CodingCat::EmacsMule, "emacs-mule"),
        (CodingCat::RawText, "raw-text"),
        (CodingCat::Iso7Tight, "iso-2022-jp"),
        (CodingCat::Iso81, "in-is13194-devanagari"),
        (CodingCat::Iso82, "chinese-iso-8bit"),
        (CodingCat::Utf8Auto, "utf-8-auto"),
        (CodingCat::Utf8Sig, "utf-8-with-signature"),
        (CodingCat::Utf16Auto, "utf-16"),
        (CodingCat::Utf16Be, "utf-16be-with-signature"),
        (CodingCat::Utf16Le, "utf-16le-with-signature"),
        (CodingCat::Utf16BeNosig, "utf-16be"),
        (CodingCat::Utf16LeNosig, "utf-16le"),
        (CodingCat::Sjis, "japanese-shift-jis"),
        (CodingCat::Big5, "chinese-big5"),
        (CodingCat::Undecided, "undecided"),
    ];
    let mut cat_system: [Option<SymId>; CODING_CAT_MAX] = [None; CODING_CAT_MAX];
    let mut priorities = Vec::new();
    for (cat, name) in order {
        cat_system[cat as usize] = Some(intern(name));
        priorities.push(cat as usize);
    }
    // ccl has no bound coding system; append it (and any other) in enum order.
    for cat in 0..CODING_CAT_MAX {
        if !priorities.contains(&cat) {
            priorities.push(cat);
        }
    }
    let mgr = CodingSystemManager::new();
    neovm_text::test_support::authority::coding_detection_bindings(
        priorities,
        cat_system,
        CharsetDetectionPlan::for_coding_system(
            &mgr,
            intern("iso-latin-1"),
            neovm_value::tagged::value::SymbolMode::Bare,
        ),
    )
}

fn detect_list(bytes: &[u8]) -> Vec<String> {
    detect_list_mb(bytes, bytes.len(), false)
}

fn detect_list_mb(bytes: &[u8], src_chars: usize, multibytep: bool) -> Vec<String> {
    let bindings = gnu_detect_state();
    let v = detect_categories(
        &bindings,
        bytes,
        src_chars,
        multibytep,
        false,
        SourceBlock::Last,
        false,
    );
    list_to_vec(&v)
        .unwrap()
        .iter()
        .map(|s| resolve_sym(s.as_bare_symbol_id().unwrap()).to_string())
        .collect()
}

/// Register the `japanese-jisx0208` charset (dim-2, ISO final 'B') in the bare
/// test registry so the iso-2022 escape detector recognizes the `$B`
/// designation, mirroring the booted runtime.
fn register_jisx0208_for_detect() {
    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol("japanese-jisx0208");
    args[1] = Value::fixnum(2); // dimension
    args[2] = Value::vector(vec![
        Value::fixnum(33),
        Value::fixnum(126),
        Value::fixnum(33),
        Value::fixnum(126),
    ]); // 94x94 code-space
    args[5] = Value::fixnum(66); // iso-final-char 'B'
    neovm_text::emacs_core::charset::builtin_define_charset_internal(args).unwrap();
}

const ISO7_DESIGNATION: &[u8] = b"\x1b$B$3\x1b(B";

fn detect_highest_with_front(front: Option<CodingCat>) -> String {
    let mut bindings = gnu_detect_state();
    if let Some(cat) = front {
        let c = cat as usize;
        neovm_text::test_support::authority::with_coding_detection_priorities_mut_for_test(
            &mut bindings,
            |priorities| {
                priorities.retain(|&x| x != c);
                priorities.insert(0, c);
            },
        );
    }
    let v = detect_categories(
        &bindings,
        ISO7_DESIGNATION,
        6,
        false,
        true,
        SourceBlock::Last,
        false,
    );
    resolve_sym(v.as_bare_symbol_id().unwrap()).to_string()
}

fn fmt(eval: &mut crate::emacs_core::eval::Context, src: &str) -> String {
    neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(src))
}

/// `crate::test_utils::new_context()` is bare (no `with-temp-buffer` macro), so seed the current
/// buffer directly: erase it, insert `seed`, then evaluate `expr`.
fn fmt_buf(eval: &mut crate::emacs_core::eval::Context, seed: &str, expr: &str) -> String {
    let src = format!("(progn (erase-buffer) (insert {seed}) {expr})");
    neovm_eval::emacs_core::print::format_eval_result(&eval.eval_str(&src))
}
