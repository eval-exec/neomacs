//! Complex combo batch 316 — `undo` engine ultimate: `undo-boundary`,
//! `buffer-undo-list` inspection, `undo-amalgamating-change`,
//! `undo` chain with text-property changes, `buffer-disable-undo`/
//! `buffer-enable-undo` toggling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx316_undo_list_capture_and_inspect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 nil \"alpha beta\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (let (before)
    (insert "alpha")
    (setq before (length buffer-undo-list))
    (insert " beta")
    (list before
          (> (length buffer-undo-list) before)
          (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_boundary_creates_nil_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 t (t . 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "a")
  (let ((before (length buffer-undo-list)))
    (undo-boundary)
    (let ((after (length buffer-undo-list)))
      (insert "b")
      (list before after
            (> (length buffer-undo-list) after)
            (nth after buffer-undo-list)))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_chain_multiple_steps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"123\" \"1\" \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "1")
  (undo-boundary)
  (insert "2")
  (undo-boundary)
  (insert "3")
  (let ((after-3 (buffer-string)))
    (undo)
    (let ((after-undo-1 (buffer-string)))
      (undo)
      (let ((after-undo-2 (buffer-string)))
        (list after-3 after-undo-1 after-undo-2)))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_after_insert_and_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"o world\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (undo-boundary)
  (delete-region 1 5)
  (let ((before-undo (buffer-string)))
    (undo)
    (list before-undo (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_amalgamating_change_combines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (let ((before (length buffer-undo-list)))
        (undo-amalgamating-change
          (insert "a")
          (insert "b")
          (insert "c"))
        (list (> (length buffer-undo-list) before)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_with_text_property_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face nil) nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (undo-boundary)
  (put-text-property 1 5 'face 'bold)
  (let ((before-undo (text-properties-at 1)))
    (undo)
    (list before-undo (text-properties-at 1) (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_buffer_disable_enable_undo_toggling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 1 \"first no-undo-1  post\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "first")
  (let ((initial-undo-len (length buffer-undo-list)))
    (buffer-disable-undo)
    (insert " no-undo-1 ")
    (buffer-enable-undo)
    (insert " post")
    (list initial-undo-len
          (length buffer-undo-list)
          (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_after_multiple_unrelated_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "first")
  (goto-char 1)
  (insert "X")
  (goto-char (point-max))
  (insert "Y")
  (let ((after-both (buffer-string)))
    (undo)
    (list after-both (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_limit_setting_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (integerp undo-limit)
      (integerp undo-strong-limit)
      (integerp (or undo-outer-limit 0))
      (> undo-limit 0)
      (> undo-strong-limit 0))
"##,
        expect,
    )
}

#[test]
fn div_cx316_undo_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Undo mega test buffer content here")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 10))
        (ov (make-overlay 4 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (narrow-to-region 2 25)
    (delete-region 5 9)
    (insert "INSERTED")
    (put-text-property 5 12 'face 'underline)
    (let ((before-undo (list (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1)
                             (text-properties-at 5))))
      (undo)
      (undo)
      (widen)
      (list before-undo (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1) (text-properties-at 5)))))
"##,
        expect,
    )
}
