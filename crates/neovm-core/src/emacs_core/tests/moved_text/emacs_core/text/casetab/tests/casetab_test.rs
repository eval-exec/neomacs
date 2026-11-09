use super::*;

use neovm_edit::buffer::buffer::BUFFER_SLOT_CASE_TABLE;
use neovm_value::error::LispCondition;
use neovm_value::error::expect_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::SymbolMode;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::emacs_core::casefiddle_entries::builtin_upcase_region;
use crate::emacs_core::casetab_entries::builtin_current_case_table;
use crate::emacs_core::casetab_entries::builtin_set_case_table;
use crate::emacs_core::casetab_entries::builtin_set_standard_case_table;
use crate::emacs_core::casetab_entries::builtin_standard_case_table;
use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::casetab::*;
use neovm_value::value::Value;

#[test]
fn manager_upcase_string() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.upcase_string("hello"), "HELLO");
    assert_eq!(mgr.upcase_string("Hello World"), "HELLO WORLD");
    assert_eq!(mgr.upcase_string("ABC"), "ABC");
    assert_eq!(mgr.upcase_string(""), "");
    assert_eq!(mgr.upcase_string("a1b2c3"), "A1B2C3");
}

#[test]
fn manager_downcase_string() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.downcase_string("HELLO"), "hello");
    assert_eq!(mgr.downcase_string("Hello World"), "hello world");
    assert_eq!(mgr.downcase_string("abc"), "abc");
    assert_eq!(mgr.downcase_string(""), "");
    assert_eq!(mgr.downcase_string("A1B2C3"), "a1b2c3");
}

#[test]
fn builtin_case_table_p_on_non_table() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_case_table_p(vec![Value::NIL]).unwrap().is_nil());
    assert!(
        builtin_case_table_p(vec![Value::fixnum(42)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_case_table_p(vec![Value::string("hello")])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn builtin_case_table_p_on_char_table() {
    crate::test_utils::init_test_tracing();
    // A proper char-table with case-table subtype.
    let ct = make_empty_case_table();
    assert!(builtin_case_table_p(vec![ct]).unwrap().is_t());
}

#[test]
fn builtin_case_table_p_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_case_table_p(vec![]).is_err());
    assert!(builtin_case_table_p(vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn builtin_current_case_table_returns_case_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    let result = builtin_current_case_table(&mut ctx, vec![]).unwrap();
    assert!(is_case_table(&result));
}

#[test]
fn builtin_current_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_current_case_table(&mut ctx, vec![Value::NIL]).is_err());
}

#[test]
fn builtin_standard_case_table_returns_case_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    let result = builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    assert!(is_case_table(&result));
}

#[test]
fn builtin_standard_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_standard_case_table(&mut ctx, vec![Value::NIL]).is_err());
}

#[test]
fn builtin_set_case_table_returns_arg() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    let table = make_empty_case_table();
    let result = builtin_set_case_table(&mut ctx, vec![table]).unwrap();
    assert_eq!(result, table);
}

#[test]
fn builtin_set_case_table_rejects_non_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_set_case_table(&mut ctx, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn builtin_set_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_set_case_table(&mut ctx, vec![]).is_err());
    assert!(builtin_set_case_table(&mut ctx, vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn builtin_set_standard_case_table_returns_arg() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    let table = make_empty_case_table();
    let result = builtin_set_standard_case_table(&mut ctx, vec![table]).unwrap();
    assert_eq!(result, table);
}

#[test]
fn builtin_set_standard_case_table_rejects_non_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_set_standard_case_table(&mut ctx, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn builtin_set_standard_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    assert!(builtin_set_standard_case_table(&mut ctx, vec![]).is_err());
}

#[test]
fn evaluator_case_table_roundtrip_and_isolation() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let standard = builtin_standard_case_table(&mut eval, vec![]).unwrap();
    let current = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(standard, current);

    let current_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let other_id = eval.edit.buffers.create_buffer("*case-other*");

    let custom = make_empty_case_table();
    builtin_set_case_table(&mut eval, vec![custom]).unwrap();
    let after_set = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(after_set, custom);

    eval.edit.buffers.set_current(other_id);
    let other_current = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(other_current, standard);

    eval.edit.buffers.set_current(current_id);
    let restored = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(restored, custom);
}

#[test]
fn builtin_downcase_char_uppercase() {
    crate::test_utils::init_test_tracing();
    // (downcase ?A) -> 97 (i.e., ?a)
    let result = builtin_downcase_char(vec![Value::char('A')]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_lowercase_unchanged() {
    crate::test_utils::init_test_tracing();
    // (downcase ?a) -> 97
    let result = builtin_downcase_char(vec![Value::char('a')]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_from_int() {
    crate::test_utils::init_test_tracing();
    // (downcase 65) -> 97 (65 = ?A, 97 = ?a)
    let result = builtin_downcase_char(vec![Value::fixnum(65)]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_wrong_type() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_downcase_char(vec![Value::string("A")]).is_err());
    assert!(builtin_downcase_char(vec![Value::NIL]).is_err());
}

#[test]
fn builtin_downcase_char_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_downcase_char(vec![]).is_err());
    assert!(builtin_downcase_char(vec![Value::char('A'), Value::char('B')]).is_err());
}

#[test]
fn string_roundtrip() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    let original = "Hello World";
    let upper = mgr.upcase_string(original);
    let lower = mgr.downcase_string(&upper);
    assert_eq!(lower, "hello world");
}

#[test]
fn custom_case_table_overrides_upcase_downcase_char() {
    crate::test_utils::init_test_tracing();
    // (set-case-syntax-pair ?z ?A): ?z is the uppercase, ?A the lowercase.
    // GNU oracle: (upcase ?z) => 122, (downcase ?A) => 65.
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    let up = neovm_eval::emacs_core::builtins::builtin_upcase_in_state_1(&mut ev, Value::char('z'))
        .unwrap();
    assert_eq!(up.as_int(), Some(122));
    let down =
        neovm_eval::emacs_core::builtins::builtin_downcase_in_state_1(&mut ev, Value::char('A'))
            .unwrap();
    assert_eq!(down.as_int(), Some(65));
}

#[test]
fn custom_case_table_overrides_upcase_region() {
    crate::test_utils::init_test_tracing();
    // Upcasing "Az" with the z/A pair: ?A upcases to ?z, ?z stays ?z => "zz".
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    let buffer_id = ev.edit.buffers.current_buffer_id().expect("current buffer");
    ev.edit.buffers.insert_into_buffer(buffer_id, "Az");
    super::super::casefiddle::builtin_upcase_region(
        &mut ev,
        vec![Value::fixnum(1), Value::fixnum(3)],
    )
    .expect("upcase-region");
    let buffer = ev.edit.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "zz");
}

#[test]
fn custom_case_table_overrides_upcase_downcase_string() {
    crate::test_utils::init_test_tracing();
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    // upcase "Az" => "zz" (A->z, z->z).
    let up =
        neovm_eval::emacs_core::builtins::builtin_upcase_in_state_1(&mut ev, Value::string("Az"))
            .unwrap();
    assert_eq!(up.as_utf8_str(), Some("zz"));
    // downcase "Az" => "AA" (A->A, z->A).
    let down =
        neovm_eval::emacs_core::builtins::builtin_downcase_in_state_1(&mut ev, Value::string("Az"))
            .unwrap();
    assert_eq!(down.as_utf8_str(), Some("AA"));
}

#[test]
fn custom_case_table_makes_char_equal_case_fold() {
    crate::test_utils::init_test_tracing();
    // (set-case-syntax-pair ?x ?Y): ?x/?Y become a case pair, so with
    // case-fold-search t, (char-equal ?x ?Y) => t. GNU oracle: t.
    let mut ev = ctx_with_pair('x' as i64, 'Y' as i64);
    ev.set_variable("case-fold-search", Value::T);
    let r = neovm_eval::emacs_core::builtins::builtin_char_equal_2(
        &mut ev,
        Value::char('x'),
        Value::char('Y'),
    )
    .unwrap();
    assert!(r.is_t(), "expected t, got {r:?}");

    // case-fold-search nil: exact match only.
    ev.set_variable("case-fold-search", Value::NIL);
    let r = neovm_eval::emacs_core::builtins::builtin_char_equal_2(
        &mut ev,
        Value::char('x'),
        Value::char('Y'),
    )
    .unwrap();
    assert!(r.is_nil(), "expected nil, got {r:?}");
}

#[test]
fn standard_case_table_path_is_byte_identical() {
    crate::test_utils::init_test_tracing();
    // Without a custom table, the default Unicode path must be unchanged.
    let mut ev = crate::test_utils::new_context();
    let up = neovm_eval::emacs_core::builtins::builtin_upcase_in_state_1(&mut ev, Value::char('z'))
        .unwrap();
    assert_eq!(up.as_int(), Some('Z' as i64));
    let down =
        neovm_eval::emacs_core::builtins::builtin_downcase_in_state_1(&mut ev, Value::char('A'))
            .unwrap();
    assert_eq!(down.as_int(), Some('a' as i64));
    // Non-ASCII Unicode casing still works (é <-> É).
    let up = neovm_eval::emacs_core::builtins::builtin_upcase_in_state_1(
        &mut ev,
        Value::string("héllo"),
    )
    .unwrap();
    assert_eq!(up.as_utf8_str(), Some("HÉLLO"));
    let down = neovm_eval::emacs_core::builtins::builtin_downcase_in_state_1(
        &mut ev,
        Value::string("HÉLLO"),
    )
    .unwrap();
    assert_eq!(down.as_utf8_str(), Some("héllo"));
    // char-equal still case-folds with the standard table.
    ev.set_variable("case-fold-search", Value::T);
    let r = neovm_eval::emacs_core::builtins::builtin_char_equal_2(
        &mut ev,
        Value::char('a'),
        Value::char('A'),
    )
    .unwrap();
    assert!(r.is_t());
}

#[test]
fn the_standard_case_tables_and_replace_match_case_answer_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU 31.1: every subsidiary carries the three extra slots, canon's
    // third is eqv, and `replace-match' carries the match's case over.
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let* ((st (standard-case-table))
               (up (char-table-extra-slot st 0))
               (canon (char-table-extra-slot st 1))
               (eqv (char-table-extra-slot st 2)))
          (list (aref st ?A) (aref st ?a) (aref up ?a) (aref up ?1) (aref canon ?Q)
                (aref eqv ?q) (aref eqv ?Q) (aref st ?É) (aref up ?é) (aref canon ?Ω)
                (aref st 1)
                (eq (char-table-extra-slot canon 2) eqv)
                (length (list (char-table-extra-slot up 0) (char-table-extra-slot up 2)))
                (with-temp-buffer
                  (insert "Hello World and HELLO WORLD and hello world and Émile")
                  (let ((case-fold-search t))
                    (goto-char 1)
                    (while (re-search-forward "hello world\\|émile" nil t)
                      (replace-match
                       (if (string-match-p "mile" (match-string 0)) "zoé" "bye there"))))
                  (buffer-string))))
        "#,
    );
    assert_eq!(
        result,
        "OK (97 97 65 49 113 81 113 233 201 969 1 t 2 \"Bye There and BYE THERE and bye there and Zoé\")"
    );
}

/// Install a custom case table with a single (UC, LC) pair in the current
/// buffer, returning the prepared evaluator. Mirrors `(set-case-table
/// (set-case-syntax-pair UC LC (copy-case-table (standard-case-table))))`.
#[cfg(test)]
fn ctx_with_pair(uc: i64, lc: i64) -> crate::emacs_core::eval::Context {
    let mut ev = crate::test_utils::new_context();
    let table = make_case_table_with_pair(uc, lc);
    builtin_set_case_table(&mut ev, vec![table]).expect("set-case-table");
    ev
}
