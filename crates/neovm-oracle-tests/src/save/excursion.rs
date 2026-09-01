//! Oracle parity tests for `save-excursion`, `save-restriction`,
//! `narrow-to-region`, `widen`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_restores_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 3)
                    (save-excursion
                      (goto-char (point-max))
                      (point)))"#;
    let expect = expect_test::expect![[r#""OK 12""#]];
    // save-excursion returns body value but restores point
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_excursion_point_restored_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 3)
                    (let ((inside
                           (save-excursion
                             (goto-char (point-max))
                             (point))))
                      (list inside (point))))"#;
    let expect = expect_test::expect![[r#""OK (12 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_excursion_restores_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 3)
                    (condition-case nil
                        (save-excursion
                          (goto-char (point-max))
                          (signal 'error '("boom")))
                      (error nil))
                    (point))"#;
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_save_excursion_restores_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion also restores the current buffer
    let form = r#"(with-temp-buffer
                    (rename-buffer "neovm--test-buf-A" t)
                    (let ((orig (current-buffer)))
                      (save-excursion
                        (set-buffer (get-buffer-create "neovm--test-buf-B"))
                        (current-buffer))
                      (eq orig (current-buffer))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_excursion_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "abcdefghij")
                    (goto-char 2)
                    (save-excursion
                      (goto-char 5)
                      (save-excursion
                        (goto-char 8)))
                    (point))"#;
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("2", &o, &n);
}

// ---------------------------------------------------------------------------
// narrow-to-region / widen / save-restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_to_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (narrow-to-region 1 6)
                    (list (point-min) (point-max) (buffer-string)))"#;
    let expect = expect_test::expect![[r#""OK (1 6 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_narrow_restricts_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (narrow-to-region 1 6)
                    (goto-char (point-min))
                    (list (point) (point-min) (point-max)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_widen_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (narrow-to-region 1 6)
                    (let ((narrow-max (point-max)))
                      (widen)
                      (list narrow-max (point-max))))"#;
    let expect = expect_test::expect![[r#""OK (6 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_restriction_restores_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (narrow-to-region 1 6)
                    (save-restriction
                      (widen)
                      (point-max))
                    (list (point-min) (point-max)))"#;
    let expect = expect_test::expect![[r#""OK (1 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_restriction_restores_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (narrow-to-region 1 6)
                    (condition-case nil
                        (save-restriction
                          (widen)
                          (signal 'error '("boom")))
                      (error nil))
                    (point-max))"#;
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_prop_save_restriction_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "abcdefghij")
                    (save-restriction
                      (narrow-to-region 1 5)
                      (save-restriction
                        (narrow-to-region 2 4)
                        (list (point-min) (point-max)))
                      ))"#;
    let expect = expect_test::expect![[r#""OK (2 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_narrow_search_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: narrow then search
    let form = r#"(with-temp-buffer
                    (insert "aaa-bbb-ccc-ddd")
                    (save-restriction
                      (narrow-to-region 5 11)
                      (goto-char (point-min))
                      (let ((found nil))
                        (when (re-search-forward "\\([a-z]+\\)" nil t)
                          (setq found (match-string 1)))
                        found)))"#;
    let expect = expect_test::expect![[r#""OK \"bbb\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: save-excursion + save-restriction + narrow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_restriction_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3\nline4\nline5")
                    (goto-char 3)
                    (let ((results nil))
                      (save-excursion
                        (save-restriction
                          (goto-char (point-min))
                          (forward-line 1)
                          (let ((start (point)))
                            (forward-line 2)
                            (narrow-to-region start (point)))
                          (goto-char (point-min))
                          (setq results
                                (list (point-min) (point-max)
                                      (buffer-substring
                                       (point-min) (point-max))))))
                      (list results (point)
                            (point-min) (point-max))))"#;
    let expect = expect_test::expect![[r#""OK ((7 19 \"line2\\nline3\\n\") 3 1 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
