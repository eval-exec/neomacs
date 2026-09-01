//! Oracle parity tests for additional missing subrs:
//! `ngettext`, `gap-size`, `process-id`, `get-pos-property`.
//!
//! These subrs were previously untested in the oracle test suite.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_ngettext_returns_singular_for_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"file\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(ngettext "file" "files" 1)"#, expect);
    assert_ok_eq("\"file\"", &o, &n);
}

#[test]
fn oracle_ngettext_returns_plural_for_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"files\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(ngettext "file" "files" 2)"#, expect);
    assert_ok_eq("\"files\"", &o, &n);
}

#[test]
fn oracle_gap_size_returns_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-gap*"))
  (integerp (gap-size)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_process_id_rejects_integer_pid_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp processp)""#]];
    // GNU src/process.c:Fprocess_id CHECK_PROCESS validates that the argument
    // is a process object; an OS PID integer is not accepted.
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
               (process-id (emacs-pid))
             (error (list (car err)
                          (caadr err)
                          (integerp (cadadr err)))))"#,
        expect,
    );
}

#[test]
fn oracle_get_pos_property_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-posprop*"))
  (erase-buffer)
  (insert "hello")
  (put-text-property 1 3 'face 'bold)
  (get-pos-property 2 'face))"#,
        expect,
    );
    assert_ok_eq("bold", &o, &n);
}
