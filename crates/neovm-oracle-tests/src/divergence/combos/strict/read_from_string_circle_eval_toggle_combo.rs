//! Strict combo oracle probes, batch 353: read-from-string with read-circle /
//! read-eval toggles. Circular read with read-circle, hash-eval forms with
//! read-eval on/off, and read-quoted-char.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_read_from_string_circle_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((read-circle t))
  (list (prin1-to-string (read-from-string "#1=(a . #1#)"))
        (multiple-value-bind (val pos) (read-from-string "#1=(x) #1#")
          (list val pos))
        (condition-case err
            (let ((read-circle nil))
              (read-from-string "#1=(a . #1#)"))
          (invalid-read-syntax (cons 'caught (cadr err)))
          (error (cons 'other (car err))))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_read_eval_toggle_hash_dot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (let ((read-eval t)) (read-from-string "#.(+ 1 2)"))
      (condition-case err
          (let ((read-eval nil)) (read-from-string "#.(+ 1 2)"))
        (error (car err)))
      (condition-case err
          (let ((read-eval nil)) (read-from-string "#.(shell-command \"rm -rf /\")"))
        (error (car err))))
"##;
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#.\")""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_read_multiple_forms_sequential_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s "(a) (b) (c) 42 \"str\""))
  (multiple-value-bind (v1 p1) (read-from-string s)
    (multiple-value-bind (v2 p2) (read-from-string s nil p1)
      (list v1 p1 v2 p2 (substring s p2)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
