//! Divergence tests: undo deep part 2 - buffer-undo-tree, undo limits, redo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_boundary_amalgamation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (nil (4 . 7) nil (1 . 4) (t . 0)) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "ABC")
  (undo-boundary)
  (insert "DEF")
  (undo-boundary)
  (insert "GHI")
  (let ((len (length buffer-undo-list)))
    (list (> len 0)
          (member nil buffer-undo-list)
          len)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_after_multiple_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AAA\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "AAA")
  (undo-boundary)
  (insert "BBB")
  (undo-boundary)
  (insert "CCC")
  (undo)
  (list (buffer-string) buffer-undo-list)
  (undo)
  (list (buffer-string))
  (undo)
  (list (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_with_markers_tracked() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "ABCDEFGH")
  (let ((m (point-marker)))
    (goto-char 3)
    (undo-boundary)
    (delete-region 3 6)
    (list (marker-position m) (buffer-string))
    (undo)
    (list (marker-position m) (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_in_narrowed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAABBBCCCDDDEEE")
  (narrow-to-region 4 13)
  (setq buffer-undo-list nil)
  (goto-char 5)
  (insert "XXX")
  (list (buffer-string))
  (undo)
  (list (buffer-string))
  (widen)
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_undo_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp undo-limit)
  (integerp undo-strong-limit)
  (> undo-limit 0)
  (> undo-strong-limit 0)
  (integerp undo-outer-limit))"#,
        expect,
    );
}

#[test]
fn divergence_undo_in_read_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello\" t ((1 . 6) (t . 0)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "Hello")
  (setq buffer-read-only t)
  (list (buffer-string)
        buffer-read-only
        buffer-undo-list))"#,
        expect,
    );
}

#[test]
fn divergence_undo_after_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "foo bar baz")
  (goto-char 1)
  (re-search-forward "bar")
  (replace-match "quux")
  (list (buffer-string))
  (undo)
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_undo_list_torture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (dotimes (i 5)
    (goto-char (point-max))
    (insert (number-to-string i))
    (undo-boundary))
  (let ((initial-string (buffer-string)))
    (dotimes (_ 5)
      (undo))
    (list initial-string
          (buffer-string)
          (= (point-max) (point-min))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_nil_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((nil (1 . 4) (t . 0)) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "ABC")
  (undo-boundary)
  (insert "DEF")
  (let ((has-boundary (memq nil buffer-undo-list)))
    (list has-boundary
          (consp has-boundary))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_only_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "Hello")
  (goto-char 3)
  (insert "X")
  (list (buffer-string))
  (undo)
  (list (buffer-string))
  (undo)
  (list (buffer-string))
  (= (point-min) (point-max)))"#,
        expect,
    );
}
