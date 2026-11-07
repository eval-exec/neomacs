//! Oracle parity for `find-buffer` reading VARIABLE per candidate buffer.
//!
//! GNU `Ffind_buffer` (src/buffer.c:544-555) walks every live buffer and
//! compares `Fbuffer_local_value (variable, buf)` (buffer.c:551).
//! `Fbuffer_local_value` (src/buffer.c:1377-1413) reads THAT buffer's
//! `local_var_alist` when the symbol is localized and falls back to
//! `Fdefault_value` only when that buffer has no local binding (buffer.c:1399);
//! for a Buffer_Obj forwarder it reads the per-buffer slot, else
//! `Fdefault_value` (buffer.c:1408). Two buffers with distinct setq-local
//! values plus a live default pin the per-buffer contract.
//!
//! Expect recorded from GNU 31.1 with NEOVM_ORACLE_MODE=refresh
//! UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_find_buffer_reads_variable_per_candidate_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-default gdm-fb-var 'gdm-default)
  (set gdm-fb-var 'gdm-default)
  (dolist (b (buffer-list))
    (with-current-buffer b (setq-local gdm-fb-var 'other)))
  (let ((b1 (generate-new-buffer " gdm-fb-one"))
        (b2 (generate-new-buffer " gdm-fb-two")))
    (with-current-buffer b2 (setq-local gdm-fb-var 'local))
    (prog1 (list (buffer-name (find-buffer 'gdm-fb-var 'local))
                 (buffer-name (find-buffer 'gdm-fb-var 'gdm-default))
                 (find-buffer 'gdm-fb-var 'never-anywhere))
      (kill-buffer b1)
      (kill-buffer b2))))
"#;
    let expect = expect_test::expect![[r#""OK (\" gdm-fb-two\" \" gdm-fb-one\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// GNU `Ffind_buffer` walks the live buffers in buffer-list order
// (`FOR_EACH_LIVE_BUFFER`, src/buffer.c:544-555) and compares
// `Fbuffer_local_value (variable, buf)` (buffer.c:551). It does not start with
// the current buffer, and a dynamic `let` is visible to Fdefault_value when
// specbind stores in place (PLAINVAL) or through LET_DEFAULT.

#[test]
fn oracle_prop_find_buffer_sees_a_dynamic_let_of_a_plain_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((plain 'x))
  (buffer-name (find-buffer 'plain 'x)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable plain)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_find_buffer_default_value_wins_in_buffer_list_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (defvar-local gdm-fb-order-var 'x)
  (buffer-name (find-buffer 'gdm-fb-order-var 'x)))
"#;
    let expect = expect_test::expect![[r#""OK \"*scratch*\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_find_buffer_does_not_start_with_the_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((fbt-b (get-buffer-create "fbt-b")))
  (with-current-buffer fbt-b
    (defvar-local gdm-fb-order2-var 'x))
  (with-current-buffer fbt-b
    (buffer-name (find-buffer 'gdm-fb-order2-var 'x))))
"#;
    let expect = expect_test::expect![[r#""OK \"*scratch*\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// The dynamic-let case with an explicitly empty lexical environment: the
// harness itself evaluates forms with lexical-binding on (where a plain `let`
// of a non-special symbol is lexical and the symbol stays void), so this form
// pins the DYNAMIC binding GNU's Fdefault_value sees for a PLAINVAL symbol.
#[test]
fn oracle_prop_find_buffer_sees_a_dynamic_let_with_no_lexical_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(eval '(let ((plain 'x)) (buffer-name (find-buffer 'plain 'x))) nil)
"#;
    let expect = expect_test::expect![[r#""OK \"*scratch*\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
