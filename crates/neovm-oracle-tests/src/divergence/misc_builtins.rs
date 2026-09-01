//! Divergence tests: miscellaneous Emacs Lisp builtins not yet covered.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_yes_or_no_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'yes-or-no-p)
  (fboundp 'y-or-n-p)
  (fboundp 'read-char-choice))"#,
        expect,
    );
}

#[test]
fn divergence_random_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp (random))
  (>= (random 100) 0)
  (< (random 100) 100)
  (integerp (random 1000)))"#,
        expect,
    );
}

#[test]
fn divergence_copy_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (((a . 1) (b . 2) (c . 3)) ((a . 1) (b . 99) (c . 3)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((orig '((a . 1) (b . 2) (c . 3)))
         (copy (copy-alist orig)))
  (setcdr (assoc 'b copy) 99)
  (list orig copy))"#,
        expect,
    );
}

#[test]
fn divergence_copy_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((orig '(1 2 3))
         (copy (copy-sequence orig)))
  (list (equal orig copy)
        (not (eq orig copy))
        (= (length copy) 3)))"#,
        expect,
    );
}

#[test]
fn divergence_copy_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((orig '(a (b (c d)) e))
         (copy (copy-tree orig)))
  (list (equal orig copy)
        (not (eq (cadr orig) (cadr copy)))))"#,
        expect,
    );
}

#[test]
fn deficiency_equal_including_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (equal-including-properties "abc" "abc")
  (equal-including-properties
    (propertize "abc" 'face 'bold)
    "abc")
  (equal-including-properties
    (propertize "abc" 'face 'bold)
    (propertize "abc" 'face 'bold)))"#,
        expect,
    );
}

#[test]
fn divergence_plist_member() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((b 2 c 3) nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '(a 1 b 2 c 3)))
  (list (plist-member pl 'b)
        (plist-member pl 'z)
        (not (plist-member pl 'z))))"#,
        expect,
    );
}

#[test]
fn divergence_format_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'format-mode-line)
  (stringp (format-mode-line mode-line-format))
  (> (length (format-mode-line mode-line-format)) 0))"#,
        expect,
    );
}

#[test]
fn divergence_accessible_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'accessible-keymaps)
  (fboundp 'where-is-internal)
  (fboundp 'describe-bindings))"#,
        expect,
    );
}

#[test]
fn divergence_local_key_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (keymapp (current-local-map))
  (keymapp (current-global-map))
  (or (null (current-local-map))
      (keymapp (current-local-map))))"#,
        expect,
    );
}
