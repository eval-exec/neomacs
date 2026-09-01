//! Divergence tests: register, bookmark stubs, and narrow/widen edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_point_to_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 #<marker at 5 in *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (goto-char 5)
  (point-to-register ?a)
  (goto-char 1)
  (jump-to-register ?a)
  (list (point)
        (get-register ?a)))"#,
        expect,
    );
}

#[test]
fn divergence_copy_to_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello\" \"Hello World!\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World!")
  (copy-to-register ?r 1 6)
  (list (get-register ?r)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_insert_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello WorldHello!\" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World!")
  (copy-to-register ?r 1 6)
  (goto-char 12)
  (insert-register ?r)
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_register_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 \"hello\" (a b c))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (set-register ?x 42)
  (set-register ?y "hello")
  (set-register ?z '(a b c))
  (list (get-register ?x)
        (get-register ?y)
        (get-register ?z)))"#,
        expect,
    );
}

#[test]
fn divergence_narrow_to_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 19 \"ine2\\nline3\\n\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\nline4\nline5")
  (goto-char 8)
  (push-mark)
  (forward-line 2)
  (narrow-to-region 8 (point))
  (list (point-min)
        (point-max)
        (buffer-string)
        (buffer-narrowed-p)))"#,
        expect,
    );
}

#[test]
fn divergence_widen_after_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 18 \"line1\\nline2\\nline3\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3")
  (narrow-to-region 7 12)
  (list (point-min) (point-max) (buffer-string) (buffer-narrowed-p))
  (widen)
  (list (point-min) (point-max) (buffer-string) (buffer-narrowed-p)))"#,
        expect,
    );
}

#[test]
fn divergence_narrow_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((m1 (set-marker (make-marker) 3))
        (m2 (set-marker (make-marker) 8)))
    (narrow-to-region 4 7)
    (list (point-min) (point-max)
          (marker-position m1)
          (marker-position m2)
          (buffer-string))
    (widen)
    (list (marker-position m1)
          (marker-position m2))))"#,
        expect,
    );
}

#[test]
fn divergence_bookmark_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'bookmark-set)
  (fboundp 'bookmark-jump)
  (fboundp 'bookmark-all-names)
  (fboundp 'bookmark-load))"#,
        expect,
    );
}

#[test]
fn divergence_fringe_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-fringe-mode)
  (fboundp 'fringe-columns)
  (boundp 'overflow-newline-into-fringe)
  (booleanp overflow-newline-into-fringe))"#,
        expect,
    );
}

#[test]
fn divergence_scroll_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'scroll-up)
  (fboundp 'scroll-down)
  (fboundp 'scroll-left)
  (fboundp 'scroll-right)
  (fboundp 'recenter))"#,
        expect,
    );
}
