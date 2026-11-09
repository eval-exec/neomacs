use super::*;

use LispSyntaxObjectReuse::{CanonicalBare, Fresh};
use neovm_edit::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_edit::emacs_core::textprop::CharPropertyResolver;
use neovm_text::emacs_core::regex_emacs::BoundarySyntaxMode;
use neovm_text::emacs_core::regex_emacs::SyntaxAssertion;
use neovm_text::emacs_core::syntax::parse_loop::{
    Entry, Plain, SafePositionRecorder, ScanEnd, ScanFinish, run_parse_loop,
};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::{GnuStringCharPos, LispString, StringCharacterCountKind};
use neovm_value::symbol::CanonicalAccess;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use strum::{EnumString, IntoStaticStr};

use crate::emacs_core::syntax_entries::builtin_backward_prefix_chars;
use crate::emacs_core::syntax_entries::builtin_forward_comment;
use neovm_edit::buffer::buffer::{Buffer, BufferId};
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;
use neovm_value::value::list_to_vec;

use neovm_edit::buffer::{CharPos0, LispCharPos1};

use neovm_text::emacs_core::syntax::*;
use neovm_text::test_support::authority::reset_syntax_table_decodes_for_test;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::value::eq_value;

#[test]
fn builtin_string_to_syntax_at_returns_nil() {
    crate::test_utils::init_test_tracing();
    let out = builtin_string_to_syntax(vec![Value::string("@")]).unwrap();
    assert_eq!(out, Value::NIL);
}

#[test]
fn char_syntax_accepts_full_emacs_character_codes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    // GNU Emacs `CHAR_VALID_P` accepts integer character codes through
    // MAX_CHAR (0x3FFFFF), including values that are not Unicode scalar
    // values.  `char-syntax` indexes the syntax table by that integer
    // code instead of converting through a host character type.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x20_0220)]).unwrap(),
        Value::char('w')
    );

    let err = builtin_char_syntax(&mut eval, vec![Value::fixnum(0x40_0000)])
        .expect_err("out-of-range character code should signal");
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn char_syntax_unibyte_buffer_maps_through_make_char_multibyte() {
    // GNU `Fchar_syntax`: in a unibyte buffer the character is mapped through
    // `make_char_multibyte` before the syntax lookup. Bytes 0x80-0xFF become
    // their eight-bit (word) character; codes >= 0x100 map past MAX_CHAR to an
    // invalid character whose syntax is the default whitespace.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    // Multibyte (default): a BMP CJK character is a word constituent.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x4e16)]).unwrap(),
        Value::char('w')
    );

    let id = eval.edit.buffers.current_buffer_id().unwrap();
    eval.edit
        .buffers
        .set_buffer_multibyte_flag(id, false)
        .unwrap();
    // CJK and other >= 0x100 codes collapse to whitespace.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x4e16)]).unwrap(),
        Value::char(' ')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x100)]).unwrap(),
        Value::char(' ')
    );
    // ASCII keeps its real syntax; high bytes become eight-bit word chars.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(b'a' as i64)]).unwrap(),
        Value::char('w')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(b'.' as i64)]).unwrap(),
        Value::char('.')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(200)]).unwrap(),
        Value::char('w')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(255)]).unwrap(),
        Value::char('w')
    );
}

#[test]
fn builtin_skip_syntax_forward_limit_uses_char_positions_for_multibyte_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("éézz");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let moved =
        builtin_skip_syntax_forward(&mut eval, vec![Value::string("w"), Value::fixnum(3)]).unwrap();
    assert_eq!(moved, Value::fixnum(2));
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        3
    );
}

#[test]
fn builtin_skip_syntax_forward_limit_stays_absolute_under_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("aéézz");
        buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            1,
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let moved =
        builtin_skip_syntax_forward(&mut eval, vec![Value::string("w"), Value::fixnum(4)]).unwrap();
    assert_eq!(moved, Value::fixnum(2));
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        4
    );
}

#[test]
fn sexp_motion_respects_parse_sexp_ignore_comments_for_line_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "(a ; comment with )\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(20)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(20)
    );
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 20);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(24)
    );
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 24);
}

#[test]
fn sexp_motion_ignores_unmatched_open_paren_inside_line_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "(a ; unmatched (\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(21)
    );

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn sexp_scanners_treat_eof_comments_as_one_forward_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, ";foo");
    install_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(5)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(5)
    );
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 5);

    replace_current_buffer_text(&mut eval, "/*x");
    install_c_block_comment_syntax(&mut eval);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(4)
    );
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 4);
}

#[test]
fn backward_sexp_motion_respects_parse_sexp_ignore_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "(a ; comment with )\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(24), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(24), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    builtin_backward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 1);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(24), Value::fixnum(-1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
    match builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(24), Value::fixnum(-1), Value::fixnum(0)],
    )
    .kinded()
    {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }

    replace_current_buffer_text(&mut eval, "(a /* comment with ( */ b)");
    install_c_block_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(27), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(27), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(27), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(20)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(27), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(20)
    );
}

#[test]
fn backward_sexp_motion_skips_nested_block_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let text = "(a /* outer ( /* inner */ after */ b)";
    let end = text.len() as i64 + 1;
    replace_current_buffer_text(&mut eval, text);
    install_nestable_c_block_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(end), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    builtin_backward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

#[test]
fn backward_comment_skip_requires_matching_comment_start() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "ab");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('a' as i64), Value::string("w 3")],
    )
    .expect("install first two-char comment-end syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('b' as i64), Value::string("w 4")],
    )
    .expect("install second two-char comment-end syntax");

    let buf = eval.edit.buffers.current_buffer().expect("current buffer");
    let entry = SyntaxTable::for_buffer(buf)
        .get_entry('b')
        .expect("syntax entry for b");
    assert_eq!(entry.class, SyntaxClass::Word);
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_END_SECOND));
    assert_eq!(
        maybe_skip_comment_backward(
            buf,
            2,
            SyntaxProperties::Ignore,
            entry.class,
            entry.flags,
            CommentEndEscapePolicy::default(),
        ),
        None
    );
}

#[test]
fn sexp_motion_respects_parse_sexp_ignore_comments_for_block_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "(a /* comment with ) */ b)");
    install_c_block_comment_syntax(&mut eval);

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(21)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(21)
    );

    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(27)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(27)
    );

    replace_current_buffer_text(&mut eval, "(a /* comment with ) b)");
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn doom_cli_style_struct_comment_does_not_break_forward_sexp() {
    crate::test_utils::init_test_tracing();
    let text = "(cl-defstruct doom-cli-context\n  ;; A session-specific ID of the current context (defaults to number\n  (pid (if-let* ((pid (getenv \"__DOOMPID\")))\n           (string-to-number pid)\n         (emacs-pid))))\n(setq after t)";
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, text);
    install_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let expected_point = text.find("\n(setq after t)").expect("marker") as i64 + 1;
    assert_eq!(current_point_lisp_pos(&eval), expected_point);
}

#[test]
fn standard_quote_and_escape_entries_are_not_canonical_bare_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    for character in [34, 92] {
        let result = eval
            .eval_str(&format!(
                "(eq (char-table-range (standard-syntax-table) {character}) \
                     (string-to-syntax (string {character})))"
            ))
            .unwrap();
        assert!(
            result.is_nil(),
            "standard syntax entry for character {character} must be fresh"
        );
    }
}

#[test]
fn matching_paren_basics_and_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('(' as i64)]).unwrap(),
        Value::char(')')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum(']' as i64)]).unwrap(),
        Value::char('[')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::char('}')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('a' as i64)]).unwrap(),
        Value::NIL
    );

    let custom = builtin_copy_syntax_table(vec![builtin_standard_syntax_table(vec![]).unwrap()])
        .expect("copy standard syntax table");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('{' as i64), Value::string("_"), custom],
    )
    .expect("make brace a symbol constituent");
    builtin_set_syntax_table(&mut eval, vec![custom]).expect("set current syntax table");
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::char('_')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::NIL
    );

    match builtin_matching_paren(&mut eval, vec![Value::string("(")]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("characterp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_matching_paren(&mut eval, vec![]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.first(), Some(&Value::symbol("matching-paren")));
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn syntax_table_eval_returns_char_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let table = builtin_syntax_table(&mut eval, vec![]).unwrap();
    let is_ct = neovm_text::emacs_core::chartable::builtin_char_table_p(vec![table]).unwrap();
    assert_eq!(is_ct, Value::T);
    let subtype =
        neovm_text::emacs_core::chartable::builtin_char_table_subtype(vec![table]).unwrap();
    assert_eq!(subtype, Value::symbol("syntax-table"));
}

#[test]
fn set_syntax_table_validates_and_returns_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let table = builtin_make_syntax_table(vec![]).unwrap();
    let out = builtin_set_syntax_table(&mut eval, vec![table]).unwrap();
    assert_eq!(out, table);

    match builtin_set_syntax_table(&mut eval, vec![Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("syntax-table-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn syntax_table_and_standard_default_to_same_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let current = builtin_syntax_table(&mut eval, vec![]).unwrap();
    let standard = builtin_standard_syntax_table(vec![]).unwrap();
    assert!(eq_value(&current, &standard));
}

#[test]
fn set_syntax_table_updates_current_buffer_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let custom = builtin_make_syntax_table(vec![]).unwrap();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum(';' as i64), Value::string("<"), custom],
    )
    .unwrap();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('\n' as i64), Value::string(">"), custom],
    )
    .unwrap();
    let current_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let other_id = eval.edit.buffers.create_buffer("*syntax-other*");

    let out = builtin_set_syntax_table(&mut eval, vec![custom]).unwrap();
    assert_eq!(out, custom);
    let current = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert_eq!(current, custom);

    eval.edit.buffers.set_current(other_id);
    let other = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert!(!eq_value(&other, &custom));

    eval.edit.buffers.set_current(current_id);
    let restored = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert_eq!(restored, custom);
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(';' as i64)]).unwrap(),
        Value::char('<')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum('\n' as i64)]).unwrap(),
        Value::char('>')
    );
}

#[test]
fn forward_comment_skips_whitespace_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("  foo");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(out, Value::NIL);
    let point_1 = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 3);
}

#[test]
fn skip_syntax_forward_honors_complement_marker() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("  hello");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let out = builtin_skip_syntax_forward(&mut eval, vec![Value::string("^ w")]).unwrap();
    assert_eq!(out, Value::fixnum(0));
    let point_1 = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 1);
}

#[test]
fn forward_comment_two_char_end_style_uses_first_ender_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry('/', string_to_syntax(". 124b").unwrap());
        tbl.modify_syntax_entry('*', string_to_syntax(". 23").unwrap());
        tbl.modify_syntax_entry('\n', string_to_syntax("> b").unwrap());
        buf.insert("code /* block comment */ more // line comment\nrest");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get() + 5,
        ));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(out, Value::T);
    let point_1 = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 25);
}

#[test]
fn forward_comment_closes_haskell_nested_comment_when_ender_starts_with_comment_class() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "{-# pragma #-}module");
    install_haskell_nested_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 15);
}

/// GNU `forw_comment` gives an ender precedence over an opener while already
/// inside a comment.  A nested token with both roles must therefore close the
/// current level rather than opening levels until end-of-buffer.
#[test]
fn forward_comment_gives_dual_role_nested_marker_ender_precedence() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "|| body ||");
    install_dual_role_nested_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// GNU processes a nested single-character opener before asking whether the
/// same character also starts a two-character ender.  With both roles on the
/// first character, the depth increment and decrement cancel instead of
/// closing the outer comment.
#[test]
fn forward_comment_preserves_single_opener_before_pair_ender_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "|| body ||");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('|' as i64), Value::string("< 1234n")],
    )
    .expect("install nested single-opener and dual-role pair syntax");

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// Two-character comment entry is one token in GNU: its pair flags override
/// the first character's base `<` syntax and carry the pair's nestedness.
#[test]
fn forward_comment_classifies_two_char_opener_before_base_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "~^ body !?");
    for (ch, syntax) in [('~', "< 1"), ('^', ". 2n"), ('!', ". 3"), ('?', ". 4n")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

#[test]
fn forward_comment_fence_honors_comment_end_escape_policy() {
    crate::test_utils::init_test_tracing();

    for (policy, expected_point) in [(Value::NIL, 5), (Value::T, 7)] {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, "!a\\!b!");
        install_comment_fence_syntax(&mut eval);
        set_comment_end_can_be_escaped(&mut eval, policy);

        let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

        assert_eq!(moved, Value::T);
        assert_eq!(current_point_lisp_pos(&eval), expected_point);
    }
}

#[test]
fn backward_comment_fence_ignores_quoted_fence_candidate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "!a\\!b!");
    install_comment_fence_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

#[test]
fn backward_comment_two_char_end_style_uses_first_ender_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "code /* block comment */ more");
    install_c_block_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        // Position immediately after the two-character comment end.
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 24,
        )));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(out, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 6);
}

/// GNU recognizes the complete `/*` marker before asking `char_quoted`
/// whether its first character is escaped.  The backslash therefore prevents
/// this apparent opener from matching the later `*/`.
#[test]
fn backward_comment_rejects_escaped_two_char_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "\\/* x */");
    install_c_block_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// Once GNU recognizes a two-character opener it replaces the first
/// character's raw class with `Scomment` before checking quoting.  A raw `>`
/// class therefore cannot make an escaped opener unescapable.
#[test]
fn backward_comment_escaped_pair_opener_overrides_raw_ender_class() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "\\~^ x !?");
    for (ch, syntax) in [('~', "> 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// GNU forms the complete two-character marker before interpreting either
/// character's base class.  Escape syntax on the first character therefore
/// does not quote the second, and String syntax on the second does not hide
/// the opener from the backward walk.
#[test]
fn backward_comment_classifies_two_char_opener_before_base_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "~^ x !?");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('~' as i64), Value::string("\\ 1")],
    )
    .expect("install escape-class opener first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('^' as i64), Value::string("\" 2")],
    )
    .expect("install string-class opener second syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string(". 3")],
    )
    .expect("install ender first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('?' as i64), Value::string(". 4")],
    )
    .expect("install ender second syntax");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

#[test]
fn backward_comment_treats_first_ambiguous_two_char_delimiter_as_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "AB\n-- C\nDE\n");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        let mut table = super::SyntaxTable::isolate_for_buffer(buf);
        table.modify_syntax_entry('-', string_to_syntax(". 1234").unwrap());
        table.modify_syntax_entry('\n', string_to_syntax(">").unwrap());
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(8));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-100)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 3);
}

/// GNU's overlap guard includes a base `<` character even when the
/// `(current,right)` pair is not itself a marker.  Here `b` looks like the
/// opener for the final `>`, but it is also the second half of `ab`, which
/// already closed the earlier comment.  Forward reparse must reject it.
#[test]
fn backward_comment_reparses_base_opener_overlapping_left_ender() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "< x ab y >");
    for (ch, syntax) in [('<', "<"), ('>', ">"), ('a', ". 3"), ('b', "< 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install overlapping delimiter syntax");
    }
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// With `comment-end-can-be-escaped` non-nil, GNU does not enter
/// `back_comment` for an escaped newline ender.  It treats that newline as
/// whitespace, then stops at the quoting backslash.
#[test]
fn backward_comment_honors_escaped_comment_ender_policy() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "# x \\\n");
    install_hash_line_comment_syntax(&mut eval);
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        neovm_value::intern::intern("comment-end-can-be-escaped"),
        Value::T,
    )
    .expect("enable escaped comment enders");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 6);
}

/// When backward motion first encounters the second character of a
/// two-character ender, GNU applies `comment-end-can-be-escaped` to that
/// position.  The pair's first character can itself supply Escape syntax,
/// making the second character quoted even though the first is not.
#[test]
fn backward_comment_applies_escape_policy_to_two_char_ender_second_character() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "~^ x !?");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('~' as i64), Value::string(". 1")],
    )
    .expect("install opener first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('^' as i64), Value::string(". 2")],
    )
    .expect("install opener second syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string("\\ 3")],
    )
    .expect("install escape-class ender first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('?' as i64), Value::string(". 4")],
    )
    .expect("install ender second syntax");
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        neovm_value::intern::intern("comment-end-can-be-escaped"),
        Value::T,
    )
    .expect("enable escaped comment enders");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 8);
}

/// GNU `Fforward_comment` stops on quoted whitespace rather than silently
/// skipping it as ordinary inter-comment space.
#[test]
fn backward_comment_stops_at_quoted_whitespace() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "\\ ");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 3);
}

/// A failed two-character end marker ending in newline is consumed as one
/// whitespace-like unit.  GNU continues from the marker's first character,
/// never from the byte between the pair.
#[test]
fn backward_comment_failed_two_char_newline_ender_resumes_before_pair() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "x\n");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('x' as i64), Value::string(". 3")],
    )
    .expect("install ender-first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('\n' as i64), Value::string("> 4")],
    )
    .expect("install newline ender-second syntax");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

/// `scan_lists` always enters GNU `back_comment` after it has classified an
/// ender; unlike `Fforward_comment`, its entry is not suppressed by
/// `comment-end-can-be-escaped`.
#[test]
fn backward_sexp_comment_entry_ignores_forward_comment_escape_policy() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a # (\\\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let end = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .accessible_char_region()
        .end_lisp()
        .as_i64();
    let start = builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)])
        .expect("scan backward over escaped comment ender");

    assert_eq!(start, Value::fixnum(1));
}

#[test]
fn backward_sexp_recognizes_two_char_ender_with_quoted_first_character() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a # (\\!?");
    install_hash_line_comment_syntax(&mut eval);
    for (ch, syntax) in [('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character ender syntax");
    }
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let end = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .accessible_char_region()
        .end_lisp()
        .as_i64();
    let start = builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)])
        .expect("scan backward over quoted two-character comment ender");

    assert_eq!(start, Value::fixnum(1));

    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }
    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// GNU matches a comment delimiter's style *and* `n` flag.  The style alone
/// does not let a flat ender close a nested opener, or vice versa.
#[test]
fn backward_comment_requires_matching_nestability() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        backward_single_char_comment_with_syntax("< n", ">"),
        (Value::NIL, 6)
    );
    assert_eq!(
        backward_single_char_comment_with_syntax("<", "> n"),
        (Value::NIL, 6)
    );
    assert_eq!(
        backward_single_char_comment_with_syntax("< n", "> n"),
        (Value::T, 1)
    );
}

/// GNU `back_comment` tracks string-quote parity across its backward walk and
/// refuses a comment starter found while inside a string (`src/syntax.c`,
/// `case Scomment`).  Without that, the `#` inside `"#"` is taken for a real
/// comment start, the closing quote is swallowed, and the scan runs off the
/// beginning of the buffer.
#[test]
fn scan_lists_backward_ignores_comment_char_inside_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    //           1234 5 6  789012345 6 78 9
    //           x = { \n   a = "#"; \n }  \n
    replace_current_buffer_text(&mut eval, "x = {\n  a = \"#\";\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 19), Value::fixnum(5));
}

/// An escaped string quote must not flip the parity: GNU skips characters that
/// `char_quoted` reports as escaped before classifying them.
#[test]
fn scan_lists_backward_ignores_escaped_string_quote_before_comment_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "x = {\n  a = \"y\\\"#\\\"z\";\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 25), Value::fixnum(5));
}

/// The parity guard must not cost us genuine comments: the same motion over a
/// real `# hi` line still has to reach the opening brace.
#[test]
fn scan_lists_backward_still_crosses_a_genuine_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "x = {\n  # hi\n  a = 1;\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 24), Value::fixnum(5));
}

/// A lone string quote *inside* a real comment makes the backward walk
/// untrustworthy, so GNU falls back to re-parsing forward from a known-safe
/// point and takes the comment start from that parse.  Refusing the comment
/// outright would leave the unbalanced `"` to be scanned as a string.
#[test]
fn scan_lists_backward_reparses_forward_when_the_comment_body_holds_a_string_quote() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "x = {\n  # a \" b\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 18), Value::fixnum(5));
}

/// Two string quotes inside the comment body leave the parity even again, so
/// the backward walk reaches the `#` outside any string and never needs the
/// forward re-parse.  Pins that the parity guard is a parity guard and not a
/// blanket "a quote appeared" refusal.
#[test]
fn scan_lists_backward_accepts_a_comment_whose_body_holds_balanced_string_quotes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "x = {\n  # a \" b \" c\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray_mut()
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 22), Value::fixnum(5));
}

/// `(forward-comment -1)` over a newline whose line holds a comment character
/// inside a string finds no comment: it stops just before the newline.
#[test]
fn backward_comment_rejects_comment_start_inside_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a = \"#\";\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 9,
        )));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// The escaped-quote variant of the same rejection: the `#` is still inside the
/// string, because `\"` does not close it.
#[test]
fn backward_comment_rejects_comment_start_inside_string_with_escaped_quotes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a = \"y\\\"#\\\"z\";\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 15,
        )));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 15);
}

/// A string quote inside the comment body sends the backward walk to the
/// forward re-parse, which still reports the comment: point lands on `#`.
#[test]
fn backward_comment_finds_comment_start_through_forward_reparse() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a\n# x \" y\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 10,
        )));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 3);
}

#[test]
fn forward_comment_validates_arity_and_type() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    match builtin_forward_comment(&mut eval, vec![]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.first(), Some(&Value::symbol("forward-comment")));
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_forward_comment(&mut eval, vec![Value::symbol("x")]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("integerp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

/// Backward comment traversal with single-line (`;` ... `\n`) comments.
///
/// Buffer: "code\n;; c1\n;; c2\n;; c3\n"
/// Emacs 1-based positions:
///   1..4   "code"
///   5      \n
///   6..10  ";; c1"
///   11     \n
///   12..16 ";; c2"
///   17     \n
///   18..22 ";; c3"
///   23     \n
///
/// From point-max (24):
///   (forward-comment -1) => t, point=18  (before ";; c3")
///   (forward-comment -3) => t, point=6   (before ";; c1")
#[test]
fn forward_comment_backward_single_line_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        // Set up ; as comment start, \n as comment end
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(
            ';',
            SyntaxEntry {
                class: SyntaxClass::Comment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        tbl.modify_syntax_entry(
            '\n',
            SyntaxEntry {
                class: SyntaxClass::EndComment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        buf.insert("code\n;; c1\n;; c2\n;; c3\n");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }

    // forward-comment -1 from point-max: skip back one comment
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(out, Value::T, "forward-comment -1 should return t");
    let point_1based = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 18,
        "after -1 skip, point should be at 18 (;; c3)"
    );

    // Reset to point-max, forward-comment -3: skip back three comments
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-3)]).unwrap();
    assert_eq!(out, Value::T, "forward-comment -3 should return t");
    let point_1based = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 6,
        "after -3 skip, point should be at 6 (;; c1)"
    );
}

#[test]
fn forward_comment_keeps_style_c_distinct_from_style_a() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "@first\nsecond!code");
    install_single_character_style_c_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(
        current_point_lisp_pos(&eval),
        15,
        "the style-a newline must not terminate a style-c comment"
    );
}

#[test]
fn backward_comment_keeps_style_c_distinct_from_style_a() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "code@first\nsecond!");
    install_single_character_style_c_comment_syntax(&mut eval);
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(
        current_point_lisp_pos(&eval),
        5,
        "the style-a newline must not hide the matching style-c opener"
    );
}

/// Backward comment traversal stops on non-comment text.
///
/// Buffer: "code\n;; c1\n;; c2\n;; c3\n"
/// From point-max, (forward-comment -100) should stop at "code" boundary,
/// returning nil with point at 6 (the start of ";; c1").
/// Actually GNU returns nil at position 5 (after "code\n") since it can't
/// skip past "code".  Let me reconsider...
///
/// GNU's logic: from point-max(24), going backward:
///   Skips \n (EndComment/whitespace), then comment 3, then comment 2,
///   then comment 1. After skipping 3 comments, point is at 6 (before
///   ";; c1"). The \n at position 5 is EndComment — back_comment is
///   called, it tries to find a matching comment start before pos 5.
///   It finds no comment start (only "code"), so back_comment fails.
///   Since ch=='\n', treat as whitespace. Now at pos 4 (after "code"),
///   char_before is 'e' — class=Word, not whitespace/comment.
///   Return false → nil, point stays at 5.
///
/// Wait, that means -100 returns nil and point=5 (after skipping
/// the 3 comments but failing on the 4th).
/// Actually let me re-examine: in GNU when back_comment fails on the
/// \n and treats it as whitespace, it continues the inner loop.
/// Next char before pos 4 is 'e', class=Word → returns nil.
/// But GNU does `inc_both` at the leave label, so point = 5.
#[test]
fn forward_comment_backward_stops_at_non_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(
            ';',
            SyntaxEntry {
                class: SyntaxClass::Comment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        tbl.modify_syntax_entry(
            '\n',
            SyntaxEntry {
                class: SyntaxClass::EndComment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        buf.insert("code\n;; c1\n;; c2\n;; c3\n");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }

    // forward-comment -100 from point-max: try to skip more comments than exist
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-100)]).unwrap();
    assert_eq!(
        out,
        Value::NIL,
        "forward-comment -100 should return nil (not enough comments)"
    );
    // Point should be after "code" — at position 5 in 1-based Emacs terms
    let point_1based = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 5,
        "after failed -100 skip, point should be at 5"
    );
}

#[test]
fn backward_prefix_chars_default_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("''foo");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 2,
        )));
    }

    let out = builtin_backward_prefix_chars(&mut eval, &vec![]).unwrap();
    assert_eq!(out, Value::NIL);
    let point_1 = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 3);
}

#[test]
fn backward_prefix_chars_moves_over_prefix_flag_chars() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("''foo");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
        let entry = string_to_syntax(". p").unwrap();
        super::SyntaxTable::isolate_for_buffer(buf).modify_syntax_entry('\'', entry);
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(char_pos_to_byte(
            buf, 2,
        )));
    }

    builtin_backward_prefix_chars(&mut eval, &vec![]).unwrap();
    let point_1 = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 1);
}

#[test]
fn backward_prefix_chars_validates_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    match builtin_backward_prefix_chars(&mut eval, &vec![Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("backward-prefix-chars"))
            );
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn modify_syntax_entry_at_descriptor_inherits_parent_or_default() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('x' as i64), Value::string("@")],
    )
    .unwrap();

    let out = builtin_char_syntax(&mut eval, vec![Value::fixnum('x' as i64)]).unwrap();
    assert_eq!(out, Value::char(' '));
}

#[test]
fn scan_lists_basic_and_backward_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a b)");
    }

    let forward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(forward, Value::fixnum(6));

    let backward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(-1), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(backward, Value::NIL);

    let second_list = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(second_list, Value::NIL);
}

#[test]
fn scan_lists_depth_exits_containing_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(progn\n  (message \"x\")\n  )tail\n");
    }

    let forward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(24), Value::fixnum(1), Value::fixnum(1)],
    )
    .unwrap();
    assert_eq!(forward, Value::fixnum(27));
}

#[test]
fn scan_lists_unbalanced_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(foo (bar baz) quux)");
    }

    match builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(1)],
    )
    .kinded()
    {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Unbalanced parentheses"),
                    Value::fixnum(1),
                    Value::fixnum(21),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn syntax_after_returns_descriptor_and_nil_out_of_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("a(");
    }

    let word = builtin_syntax_after(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(
        word,
        syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Word))
    );

    let open = builtin_syntax_after(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        open,
        syntax_entry_to_value(&SyntaxEntry::with_match(SyntaxClass::Open, ')'))
    );

    let oob = builtin_syntax_after(&mut eval, vec![Value::fixnum(3)]).unwrap();
    assert_eq!(oob, Value::NIL);
}

#[test]
fn scan_sexps_basic_and_backward_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a b)");
    }

    let forward = builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap();
    assert_eq!(forward, Value::fixnum(6));

    let backward =
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(-1)]).unwrap();
    assert_eq!(backward, Value::NIL);
}

#[test]
fn scan_sexps_returns_nil_when_count_not_exhausted_at_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }

    let past_end = builtin_scan_sexps(&mut eval, vec![Value::fixnum(4), Value::fixnum(1)]).unwrap();
    assert_eq!(past_end, Value::NIL);

    let not_enough =
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(not_enough, Value::NIL);
}

#[test]
fn scan_sexps_unbalanced_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(foo (bar baz)");
    }

    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Unbalanced parentheses"),
                    Value::fixnum(1),
                    Value::fixnum(15),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn forward_sexp_unexpected_close_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    match builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Containing expression ends prematurely"),
                    Value::fixnum(1),
                    Value::fixnum(2),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn forward_sexp_stops_at_narrowing_boundary_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(aaa (bbb (ccc) ddd) eee)");
        buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(5, 20));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    builtin_forward_sexp(&mut eval, vec![Value::fixnum(3)]).unwrap();

    let point = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point, 21);
}

#[test]
fn parse_partial_sexp_baseline_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(4)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );

    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a)");
    }
    let nested =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(
        nested,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(1)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_accepts_marker_positions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }

    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let from = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(1),
        false,
    );
    let to = neovm_edit::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.edit.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );

    let state = builtin_parse_partial_sexp(&mut eval, vec![from, to]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_rejects_non_integer_positions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let err = builtin_parse_partial_sexp(&mut eval, vec![Value::make_float(1.2), Value::fixnum(1)])
        .unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(
                sig.symbol,
                neovm_value::intern::intern("wrong-type-argument")
            );
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("integer-or-marker-p"))
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn parse_partial_sexp_tracks_completed_sexp_per_nesting_level_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a (");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(1), Value::fixnum(4)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_preserves_gnu_negative_depth_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")");
    }

    let close =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(
        close,
        Value::list(vec![
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );

    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")(");
    }

    let close_then_open =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(
        close_then_open,
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(2)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_enters_single_char_line_comment_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(';', SyntaxEntry::simple(SyntaxClass::Comment));
        tbl.modify_syntax_entry('\n', SyntaxEntry::simple(SyntaxClass::EndComment));
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(";; x\n");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_closes_haskell_nested_comment_before_following_code() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "{-# pragma #-}module");
    install_haskell_nested_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(21)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(nth_value(&state, 8), Value::NIL);
}

#[test]
fn parse_partial_sexp_gives_dual_role_nested_marker_ender_precedence() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "|| body ||");
    install_dual_role_nested_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(11)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
}

#[test]
fn parse_partial_sexp_preserves_single_opener_before_pair_ender_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "|| body ||");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('|' as i64), Value::string("< 1234n")],
    )
    .expect("install nested single-opener and dual-role pair syntax");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(11)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::fixnum(1));
}

/// GNU recognizes an atomic two-character opener before applying STOPBEFORE
/// to the first character's raw base syntax.
#[test]
fn parse_partial_sexp_stopbefore_does_not_split_two_char_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "~^ body !?");
    for (ch, syntax) in [('~', "( 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(11), Value::NIL, Value::T],
    )
    .unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// GNU recognizes a pair inside `symstarted` before promoting the pending
/// atom.  A word-like raw class on the pair's first character therefore keeps
/// element 2 (the last complete sexp start) nil.
#[test]
fn parse_partial_sexp_word_class_pair_does_not_finish_pending_atom() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "a~^x!?");
    for (ch, syntax) in [('~', "w 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_crosses_multibyte_gap_at_two_char_comment_closer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let text = "(α /* β */ γ)";
    replace_current_buffer_text(&mut eval, text);
    install_c_block_comment_syntax(&mut eval);

    // Put the physical gap between the two characters of `*/` without
    // changing the logical text.  This makes the parser's current/look-ahead
    // cursor refresh its storage window while consuming an atomic pair.
    let gap_char = text
        .find("*/")
        .map(|byte| text[..byte].chars().count() + 1)
        .expect("comment closer");
    let gap_byte = {
        let buf = eval.edit.buffers.current_buffer().expect("current buffer");
        char_pos_to_byte(buf, gap_char)
    };
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(gap_byte));
        buf.insert("x");
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            gap_byte,
            gap_byte + 1,
        ));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }

    let to = text.chars().count() as i64 + 1;
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(to)]).unwrap();

    assert_eq!(nth_value(&state, 0), Value::fixnum(0));
    assert_eq!(nth_value(&state, 2), Value::fixnum(1));
    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), to);
}

#[test]
fn parse_partial_sexp_incomplete_comment_publishes_trailing_escape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* x \\");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::T);
    assert_eq!(nth_value(&state, 5), Value::T);
    assert_eq!(
        nth_value(&state, 10),
        Value::fixnum(parse_prev_syntax_int(
            SyntaxClass::Escape,
            SyntaxFlags::empty()
        ))
    );
}

#[test]
fn parse_partial_sexp_flat_incomplete_comment_hides_trailing_start_first() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* x /");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::T);
    assert_eq!(nth_value(&state, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_nested_single_ender_retains_end_first_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "<<x>");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string("< n")],
    )
    .expect("install nested single-character opener");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string("> 3n")],
    )
    .expect("install nested single-character ender with end-first flag");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::fixnum(1));
    assert_eq!(
        nth_value(&state, 10),
        Value::fixnum(parse_prev_syntax_int(
            SyntaxClass::EndComment,
            SyntaxFlags::COMMENT_END_FIRST | SyntaxFlags::COMMENT_NESTABLE,
        ))
    );
}

#[test]
fn parse_partial_sexp_consumed_comment_escape_is_not_incomplete() {
    crate::test_utils::init_test_tracing();

    for generic_fence in [false, true] {
        let mut eval = crate::test_utils::new_context();
        if generic_fence {
            replace_current_buffer_text(&mut eval, "!x\\a");
            install_comment_fence_syntax(&mut eval);
        } else {
            replace_current_buffer_text(&mut eval, "/* x \\a");
            install_c_block_comment_syntax(&mut eval);
        }
        set_comment_end_can_be_escaped(&mut eval, Value::T);
        let point_max = eval
            .edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .accessible_char_region()
            .end_lisp()
            .as_i64();

        let state =
            builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(point_max)])
                .unwrap();

        assert!(nth_value(&state, 4).is_truthy());
        assert_eq!(nth_value(&state, 5), Value::NIL);
        assert_eq!(nth_value(&state, 10), Value::NIL);
    }
}

#[test]
fn parse_partial_sexp_commentstop_preserves_entry_syntax() {
    crate::test_utils::init_test_tracing();

    for (commentstop, expected_prev_syntax) in [
        (
            Value::T,
            Value::fixnum(parse_prev_syntax_int(
                SyntaxClass::Comment,
                SyntaxFlags::COMMENT_START_FIRST,
            )),
        ),
        (Value::NIL, Value::NIL),
    ] {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, "<x");
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum('<' as i64), Value::string("< 1")],
        )
        .expect("install start-first single-character comment opener");

        let state = builtin_parse_partial_sexp(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(if commentstop.is_truthy() { 2 } else { 3 }),
                Value::NIL,
                Value::NIL,
                Value::NIL,
                commentstop,
            ],
        )
        .unwrap();

        assert_eq!(nth_value(&state, 10), expected_prev_syntax);
    }
}

#[test]
fn parse_partial_sexp_comment_resume_does_not_apply_old_quoted_as_skip() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* \\*/");
    install_c_block_comment_syntax(&mut eval);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(nth_value(&first, 5), Value::T);

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(5),
            Value::fixnum(7),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(nth_value(&resumed, 4), Value::NIL);
    assert_eq!(nth_value(&resumed, 5), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_resume_closes_a_pair_split_at_oldstate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* x */");
    install_c_block_comment_syntax(&mut eval);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::T);
    assert!(nth_value(&first, 10).as_fixnum().is_some());

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(7),
            Value::fixnum(8),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(nth_value(&resumed, 4), Value::NIL);
    assert_eq!(nth_value(&resumed, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_resume_opens_a_nested_pair_split_at_oldstate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* /* */ */");
    install_nestable_c_block_comment_syntax(&mut eval);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::fixnum(1));
    assert!(nth_value(&first, 10).as_fixnum().is_some());

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(5),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(
        nth_value(&resumed, 4),
        Value::fixnum(1),
        "the split nested opener must keep the outer comment open after the inner closer"
    );
}

#[test]
fn parse_partial_sexp_comment_resume_does_not_pair_oldstate_at_accessible_begv() {
    crate::test_utils::init_test_tracing();

    for narrowed in [false, true] {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, if narrowed { "x/" } else { "/" });
        install_c_block_comment_syntax(&mut eval);
        if narrowed {
            eval.edit
                .buffers
                .current_buffer_mut()
                .expect("current buffer")
                .narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(1, 2));
        }

        let begv = if narrowed { 2 } else { 1 };
        let oldstate = Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(parse_prev_syntax_int(
                SyntaxClass::Punctuation,
                SyntaxFlags::COMMENT_END_FIRST,
            )),
        ]);

        let resumed = builtin_parse_partial_sexp(
            &mut eval,
            vec![
                Value::fixnum(begv),
                Value::fixnum(begv + 1),
                Value::NIL,
                Value::NIL,
                oldstate,
            ],
        )
        .unwrap();

        assert_eq!(
            nth_value(&resumed, 4),
            Value::T,
            "OLDSTATE must not synthesize a delimiter before accessible BEGV"
        );
        assert_eq!(nth_value(&resumed, 10), Value::NIL);
    }
}

#[test]
fn parse_partial_sexp_consumed_two_char_closer_has_no_prev_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* x */");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(8)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(nth_value(&state, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_fence_honors_comment_end_escape_policy() {
    crate::test_utils::init_test_tracing();

    for (policy, expected_comment_state) in [(Value::NIL, Value::T), (Value::T, Value::NIL)] {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, "!a\\!b!");
        install_comment_fence_syntax(&mut eval);
        set_comment_end_can_be_escaped(&mut eval, policy);

        let state = builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)])
            .unwrap();

        assert_eq!(nth_value(&state, 4), expected_comment_state);
    }
}

/// GNU's forward parser uses the same complete delimiter identity as
/// `back_comment`: a flat ender cannot close a nested comment, and a nested
/// ender cannot close a flat one.
#[test]
fn parse_partial_sexp_requires_matching_comment_nestability() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("< n", ">"),
        Value::fixnum(1)
    );
    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("<", "> n"),
        Value::T
    );
    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("< n", "> n"),
        Value::NIL
    );
}

#[test]
fn parse_partial_sexp_commentstop_syntax_table_moves_point_across_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(';', SyntaxEntry::simple(SyntaxClass::Comment));
        tbl.modify_syntax_entry('\n', SyntaxEntry::simple(SyntaxClass::EndComment));
        buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(";; x\nfoo");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        first,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        2
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        second,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        6
    );
}

#[test]
fn parse_partial_sexp_preserves_generic_comment_fence_across_stops() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "!abc!");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string("!")],
    )
    .expect("install generic comment fence");

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 2);
    assert_eq!(
        nth_value(&first, 4),
        Value::T,
        "generic comment fences are non-nestable"
    );
    assert_eq!(nth_value(&first, 7), Value::symbol("syntax-table"));

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(6),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 6);
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
}

#[test]
fn parse_partial_sexp_preserves_c_line_comment_style_across_stops() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "// header\n#define VALUE 1\n");
    install_c_line_and_block_comment_syntax(&mut eval);

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(27),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 3);
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(
        nth_value(&first, 7),
        Value::fixnum(1),
        "GNU serializes C line-comment style b in parse-state element 7"
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(27),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        current_point_lisp_pos(&eval),
        11,
        "continued parsing must stop immediately after the style-b newline terminator"
    );
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
}

#[test]
fn parse_partial_sexp_uses_the_main_character_style_for_c_block_comment_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "/* header */\n#define VALUE 1\n");
    install_c_line_and_block_comment_syntax(&mut eval);

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(30),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 3);
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(
        nth_value(&first, 7),
        Value::NIL,
        "GNU serializes C block comments as style a"
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(30),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        current_point_lisp_pos(&eval),
        13,
        "continued parsing must stop immediately after the style-a */ terminator"
    );
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
}

#[test]
fn syntax_class_at_char_matches_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    // Pin the GNU-standard syntax-class for representative characters,
    // read from the live chartable the default buffer holds.
    let mut eval = crate::test_utils::new_context();
    let table = super::current_buffer_syntax_table_object_in_buffers(&mut eval.edit.buffers)
        .expect("buffer syntax-table");

    let cases = [
        ('a', SyntaxClass::Word),
        ('Z', SyntaxClass::Word),
        ('0', SyntaxClass::Word),
        (' ', SyntaxClass::Whitespace),
        ('\t', SyntaxClass::Whitespace),
        ('\n', SyntaxClass::Whitespace),
        ('_', SyntaxClass::Symbol),
        ('(', SyntaxClass::Open),
        (')', SyntaxClass::Close),
        ('"', SyntaxClass::StringDelim),
        ('\\', SyntaxClass::Escape),
        ('.', SyntaxClass::Punctuation),
        (';', SyntaxClass::Punctuation),
        ('\u{4e2d}', SyntaxClass::Word), // CJK: range 0x80..=0x3FFFFF is Word
        ('\u{1F600}', SyntaxClass::Word),
    ];
    for (c, expected) in cases {
        let got = super::syntax_class_at_char(&table, c);
        assert_eq!(
            got, expected,
            "class for {:?} (U+{:04X}) expected {:?}, got {:?}",
            c, c as u32, expected, got
        );
    }
}

#[test]
fn parse_partial_sexp_prefix_quote_does_not_become_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "'abc");
    install_elisp_prefix_syntax(&mut eval);
    // (with-temp-buffer (emacs-lisp-mode) (insert "'abc")
    //   (nth 2 (parse-partial-sexp (point-min) (point-max)))) => 2
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::fixnum(2));
}

#[test]
fn parse_partial_sexp_bare_prefix_quote_has_no_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "'");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "'") (nth 2 (parse-partial-sexp (point-min) (point-max))) => nil
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_other_prefix_quotes_match_gnu() {
    crate::test_utils::init_test_tracing();
    // backquote, comma and `#'` prefixes are all Squote in elisp; each yields
    // the following sexp's start as element 2, never the prefix's own position.
    // GNU: ,foo => 2, `bar => 2, #'foo => 3.
    for (text, to, expected) in [(",foo", 5, 2i64), ("`bar", 5, 2), ("#'foo", 6, 3)] {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, text);
        install_elisp_prefix_syntax(&mut eval);
        let state =
            builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(to)])
                .unwrap();
        assert_eq!(
            nth_value(&state, 2),
            Value::fixnum(expected),
            "element 2 for {text:?}"
        );
    }
}

#[test]
fn parse_partial_sexp_double_prefix_quote_has_no_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "''");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "''") (nth 2 (parse-partial-sexp (point-min) (point-max))) => nil
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_quote_continues_in_progress_symbol_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "abc'def");
    install_elisp_prefix_syntax(&mut eval);
    // A quote inside a symbol run is a constituent: `abc'def` is one symbol
    // starting at 1, so element 2 is 1 (GNU => 1).
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(8)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::fixnum(1));
}

#[test]
fn parse_partial_sexp_prefix_quote_before_paren_full_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "'abc");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "'abc") (parse-partial-sexp (point-min) (point-max))
    //   => (0 nil 2 nil nil nil 0 nil nil nil nil)
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_from_state_in_string_normalizes_unknown_start_to_minus_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "abc\" def");
    // (parse-partial-sexp (point-min) 4 nil nil
    //   (list 0 nil nil ?\" nil nil 0 nil nil nil nil)) => element 8 is -1
    let oldstate = Value::list(vec![
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::fixnum('"' as i64),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::fixnum(-1));
    // Still inside the string at TO.
    assert_eq!(nth_value(&state, 3), Value::fixnum('"' as i64));
}

#[test]
fn parse_partial_sexp_from_state_in_comment_normalizes_unknown_start_to_minus_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "abc def");
    // Resume inside a comment (element 4 = t) with element 8 nil => -1.
    let oldstate = Value::list(vec![
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::fixnum(-1));
}

#[test]
fn parse_partial_sexp_from_state_not_in_string_or_comment_keeps_start_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "abc def");
    // No string/comment in the from-state: element 8 must stay nil (no
    // spurious -1), matching GNU's gated output of `comstr_start`.
    let oldstate = Value::list(vec![
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::NIL);
}

#[test]
fn forward_word_with_boundary_table_reaches_buffer_end() {
    crate::test_utils::init_test_tracing();
    // GNU Fforward_word: when scan_words finds no further word, point still
    // moves to the accessible limit. With a find-word-boundary-function-table
    // installed (the subword-mode configuration), the boundary path used to
    // leave point stalled before trailing non-word text, turning the
    // ubiquitous (while (< (point) (point-max)) (forward-word)) idiom into an
    // infinite loop.
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "word ")
                 (let ((find-word-boundary-function-table
                        (let ((tab (make-char-table nil)))
                          (set-char-table-range tab t #'ignore)
                          tab)))
                   (goto-char (point-min))
                   (forward-word)
                   (forward-word)
                   (list (point) (point-max))))"#,
        )
        .expect("forward-word with boundary table");
    let rendered = neovm_eval::emacs_core::print::format_eval_result(&Ok(result));
    assert_eq!(rendered, "OK (6 6)");
}

/// The per-scan ASCII syntax memo must decode each distinct ASCII character at
/// most once, no matter how long the scan is.
///
/// Guards the shape that motivated it: font-lock drives `parse-partial-sexp`
/// over many SHORT ranges, so the memo is filled ON MISS rather than
/// precomputed. A regression to eager precompute (or to no memo at all) shows
/// up here as a fill count tied to the scan length instead of to the alphabet.
#[test]
fn ascii_syntax_memo_decodes_each_char_at_most_once_per_scan() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    // ~1.6k chars drawn from a small alphabet: many characters stepped over,
    // few DISTINCT ones to decode.
    let unit = "(defun f (x) (+ x 1)) ; c\n";
    let text = unit.repeat(64);
    let chars = text.chars().count();
    let distinct = {
        let mut seen = text.chars().collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    replace_current_buffer_text(&mut eval, &text);

    reset_syntax_table_decodes_for_test();
    let _ = builtin_parse_partial_sexp(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(chars as i64 + 1)],
    )
    .unwrap();
    let decodes = syntax_table_decodes_for_test();

    // Long scans (>= 256 chars) may eagerly build the flat 128-entry ASCII
    // table — a bounded, span-amortized cost that replaced the per-char memo
    // decode on the batch path (measured -88M Ir on a fontify pass). The
    // invariant is that decode cost is bounded by the ALPHABET (128 + the
    // distinct non-batched stragglers), never by the scan length.
    assert!(
        decodes <= 128 + distinct,
        "decoded {decodes} entries over a {chars}-char scan -- cost must be \
         bounded by the ASCII alphabet, not the scan length"
    );
    assert!(
        decodes * 4 < chars,
        "decoded {decodes} entries over a {chars}-char scan -- neither the \
         flat table nor the memo is holding"
    );

    // SHORT scans must keep the lazy on-miss memo: font-lock drives
    // parse-partial-sexp over many short ranges, and an eager 128-entry
    // fill measured strictly worse there (see the memo's variant table).
    reset_syntax_table_decodes_for_test();
    let _ =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(40)]).unwrap();
    let short_decodes = syntax_table_decodes_for_test();
    let short_distinct = {
        let mut seen = text.chars().take(39).collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    assert!(
        short_decodes <= short_distinct,
        "short scan decoded {short_decodes} entries for {short_distinct} \
         distinct chars -- the lazy memo must survive on short spans"
    );
}

/// A `parse-partial-sexp` RESUMED from OLDSTATE whose element 10 names a
/// comment-start-first character pairs it with the first character of the new
/// range, as GNU's `scan_sexps_forward` does at entry
/// (`in_2char_comment_start`, src/syntax.c:3265). Each case is
/// `(let ((s1 (parse-partial-sexp 1 SPLIT))) (parse-partial-sexp SPLIT END nil STOP s1 COMMENTSTOP))`
/// with the values GNU 31.1 printed (tmp/eb/pps-resume-oracle.el): the first
/// state, point after the second parse, the second state.
#[test]
fn a_resumed_parse_pairs_a_comment_opener_split_at_its_start_like_gnu() {
    crate::test_utils::init_test_tracing();
    // (text, split, end, commentstop, nested-table?, GNU's printed result)
    let cases: &[ResumedParseCase<'_>] = &[
        (
            "a /* b */ c",
            4,
            12,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 12 (0 nil 11 nil nil nil 0 nil nil nil nil))",
        ),
        (
            "a /* b */ c",
            4,
            7,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 7 (0 nil nil nil t nil 0 nil 3 nil nil))",
        ),
        (
            "a // b\nc",
            4,
            9,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 9 (0 nil 8 nil nil nil 0 nil nil nil nil))",
        ),
        (
            "a /x",
            4,
            5,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 5 (0 nil 4 nil nil nil 0 nil nil nil nil))",
        ),
        (
            "a /* b",
            4,
            5,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 5 (0 nil nil nil t nil 0 nil 3 nil nil))",
        ),
        (
            "a /* b */ c",
            4,
            12,
            Some("t"),
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 5 (0 nil nil nil t nil 0 nil 3 nil nil))",
        ),
        (
            "a /* b */ c",
            4,
            12,
            Some("syntax-table"),
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil 2818049) 5 (0 nil nil nil t nil 0 nil 3 nil nil))",
        ),
        (
            "a /* b */ c",
            3,
            12,
            None,
            false,
            "((0 nil 1 nil nil nil 0 nil nil nil nil) 12 (0 nil 11 nil nil nil 0 nil nil nil nil))",
        ),
        (
            "(* a *) b",
            2,
            9,
            None,
            true,
            "((1 1 nil nil nil nil 0 nil nil (1) 4259844) 9 (1 1 nil nil nil nil 1 nil nil (1) nil))",
        ),
        (
            "x (* (* a *) *) b",
            4,
            18,
            None,
            true,
            "((1 3 nil nil nil nil 0 nil nil (3) 4259844) 18 (1 3 17 nil nil nil 1 nil nil (3) nil))",
        ),
        (
            "a /* b */ c",
            5,
            12,
            None,
            false,
            "((0 nil 1 nil t nil 0 nil 3 nil nil) 12 (0 nil 11 nil nil nil 0 nil nil nil nil))",
        ),
    ];
    for &(text, split, end, commentstop, nested, gnu) in cases {
        let mut eval = crate::test_utils::new_context();
        replace_current_buffer_text(&mut eval, text);
        if nested {
            for (ch, desc) in [('(', "()1n"), (')', ")(4n"), ('*', ". 23n")] {
                builtin_modify_syntax_entry(
                    &mut eval,
                    vec![Value::fixnum(ch as i64), Value::string(desc)],
                )
                .expect("nested comment syntax");
            }
        } else {
            install_c_line_and_block_comment_syntax(&mut eval);
        }
        let s1 =
            builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(split)])
                .expect("first parse");
        let commentstop = match commentstop {
            None => Value::NIL,
            Some("t") => Value::T,
            Some(sym) => Value::symbol(sym),
        };
        let s2 = builtin_parse_partial_sexp(
            &mut eval,
            vec![
                Value::fixnum(split),
                Value::fixnum(end),
                Value::NIL,
                Value::NIL,
                s1,
                commentstop,
            ],
        )
        .expect("resumed parse");
        let got = format!(
            "({} {} {})",
            crate::emacs_core::print::print_value(&s1),
            current_point_lisp_pos(&eval),
            crate::emacs_core::print::print_value(&s2)
        );
        assert_eq!(
            got, gnu,
            "{text:?} split {split} end {end} commentstop {commentstop:?}"
        );
    }
}

/// Scanning BACKWARD over a comment-start character.
///
/// GNU's two `scan_lists` loops treat comment syntax asymmetrically, and the
/// asymmetry is easy to miss because the forward rule reads like the general
/// one. Forward (`while (count > 0)`) has `case Scomment: if
/// (!parse_sexp_ignore_comments) break;` -- when comments ARE ignored the body
/// was already consumed by `forw_comment`, so reaching the case at all means
/// they are not. Backward (`while (count < 0)`) has NO `case Scomment` at all:
/// a comment-start character falls to `default:` -- "Ignore whitespace,
/// punctuation, quote, endcomment" -- and is stepped over unconditionally.
///
/// That is not an oversight in GNU. Going backward a comment is entered
/// through its END (`Sendcomment` -> `back_comment`), so meeting a bare `;`
/// means point was already inside the comment body, where the `;` is just
/// text. Treating it as a sexp made `backward-sexp` stop on the semicolon
/// instead of skipping the comment and finding the form before it.
///
/// The third column is `forward-sexp`, which already agreed with GNU and is
/// pinned so the fix cannot be made by changing the forward rule.
///
/// Expectations measured under GNU Emacs 31.1 (`tmp/rr/bsexp-oracle2.el`).
#[test]
fn scanning_backward_steps_over_a_comment_start_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(mapcar
             (lambda (c)
               (let ((text (car c)) (pos (cadr c)))
                 (with-temp-buffer
                   (insert text)
                   (emacs-lisp-mode)
                   (list (progn (goto-char pos)
                                (ignore-errors (backward-sexp 1)) (point))
                         (ignore-errors (scan-sexps pos -1))
                         (progn (goto-char pos)
                                (ignore-errors (forward-sexp 1)) (point))))))
             '(("(foo)\n;; bar\n"        8)
               ("(foo)\n;; bar\n"        9)
               ("(foo)\n;; bar\n"       11)
               ("(foo)\n;; bar\n"       13)
               ("(foo)\n; bar"          10)
               ("(foo) ; bar\n"          9)
               ("(a) (b)\n;; c\n"       11)
               ("(foo)\n;;\n"            9)
               ("(foo)\n(bar)\n"        12)
               ("foo ;; bar\n"           8)
               ("(foo)\n;; a ;; b\n"    14)))"#,
    );
    assert_eq!(
        result,
        concat!(
            "OK ((1 1 14) (1 1 13) (10 10 13) (10 10 14) (9 9 12) (1 1 12) ",
            "(5 5 13) (1 1 10) (7 7 13) (1 1 11) (10 10 16))",
        )
    );
}

fn char_pos_to_byte(buf: &Buffer, char_pos: usize) -> usize {
    buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

fn current_point_lisp_pos(eval: &crate::emacs_core::eval::Context) -> i64 {
    eval.edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1
}

fn replace_current_buffer_text(eval: &mut crate::emacs_core::eval::Context, text: &str) {
    let buf = eval
        .edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer");
    buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert(text);
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.point_min_emacs_byte_pos().get(),
    ));
}

fn install_line_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum(';' as i64), Value::string("<")])
        .expect("install semicolon comment start syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install newline comment end syntax");
}

fn install_c_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('/' as i64), Value::string(". 124b")],
    )
    .expect("install C slash comment syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('*' as i64), Value::string(". 23")])
        .expect("install C star comment syntax");
}

fn install_c_line_and_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    install_c_block_comment_syntax(eval);
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string("> b")])
        .expect("install C++ line-comment end syntax");
}

fn install_nestable_c_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('/' as i64), Value::string(". 124bn")],
    )
    .expect("install nestable C slash comment syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('*' as i64), Value::string(". 23n")],
    )
    .expect("install nestable C star comment syntax");
}

fn install_haskell_nested_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('{' as i64), Value::string("(}1nb")],
    )
    .expect("install Haskell opening brace syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('}' as i64), Value::string("){4nb")],
    )
    .expect("install Haskell closing brace syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('-' as i64), Value::string("< 123")],
    )
    .expect("install Haskell dash comment syntax");
}

fn install_single_character_style_c_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('@' as i64), Value::string("< c")])
        .expect("install style-c comment start");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('!' as i64), Value::string("> c")])
        .expect("install style-c comment end");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install style-a comment end");
}

fn install_dual_role_nested_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('|' as i64), Value::string(". 1234n")],
    )
    .expect("install nested dual-role delimiter syntax");
}

fn install_comment_fence_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('!' as i64), Value::string("!")])
        .expect("install generic comment fence syntax");
}

fn set_comment_end_can_be_escaped(eval: &mut crate::emacs_core::eval::Context, enabled: Value) {
    let buffer_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        neovm_value::intern::intern("comment-end-can-be-escaped"),
        enabled,
    )
    .expect("set comment-end-can-be-escaped");
}

/// `#` starts a comment, `\n` ends one; `"` and `\` keep their standard
/// string-quote and escape syntax.  This is the three-entry table from the
/// upstream reproducer for backward sexp motion across a comment character
/// that only *appears* inside a string.
fn install_hash_line_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('#' as i64), Value::string("<")])
        .expect("install hash comment start syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install newline comment end syntax");
}

fn backward_single_char_comment_with_syntax(
    opener_syntax: &str,
    ender_syntax: &str,
) -> (Value, i64) {
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "< x >");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string(opener_syntax)],
    )
    .expect("install comment opener syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string(ender_syntax)],
    )
    .expect("install comment ender syntax");
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    (moved, current_point_lisp_pos(&eval))
}

fn scan_lists_backward_from(eval: &mut crate::emacs_core::eval::Context, from: i64) -> Value {
    builtin_scan_lists(
        eval,
        vec![Value::fixnum(from), Value::fixnum(-1), Value::fixnum(0)],
    )
    .expect("scan-lists backward")
}

fn parse_partial_sexp_comment_state_for_delimiters(
    opener_syntax: &str,
    ender_syntax: &str,
) -> Value {
    let mut eval = crate::test_utils::new_context();
    replace_current_buffer_text(&mut eval, "< x >");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string(opener_syntax)],
    )
    .expect("install parse comment opener syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string(ender_syntax)],
    )
    .expect("install parse comment ender syntax");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    nth_value(&state, 4)
}

/// Extract the nth element of a `parse-partial-sexp` result list.
fn nth_value(list: &Value, n: usize) -> Value {
    list_to_vec(list)
        .expect("parse-partial-sexp returns a list")
        .get(n)
        .cloned()
        .unwrap_or(Value::NIL)
}

/// Install the `emacs-lisp-mode` expression-prefix syntax on the current
/// buffer, making `'`, backquote, `,` and `#` Squote (class `'`) — the same
/// reassignment `lisp-mode-syntax-table` performs over the fundamental-mode
/// standard table (where these are punctuation).
fn install_elisp_prefix_syntax(eval: &mut crate::emacs_core::eval::Context) {
    let buf = eval
        .edit
        .buffers
        .current_buffer_mut()
        .expect("current buffer");
    let entry = string_to_syntax("'").unwrap();
    let mut table = super::SyntaxTable::isolate_for_buffer(buf);
    for ch in ['\'', '`', ',', '#'] {
        table.modify_syntax_entry(ch, entry);
    }
}

/// One resumed-parse oracle case: text, split, end, COMMENTSTOP, whether the
/// nested-comment syntax entries are installed, and GNU's printed result.
type ResumedParseCase<'a> = (&'a str, i64, i64, Option<&'a str>, bool, &'a str);

#[cfg(test)]
#[path = "gdn_word_motion_test.rs"]
mod gdn_word_motion;
