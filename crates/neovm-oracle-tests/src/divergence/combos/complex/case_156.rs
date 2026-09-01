//! Complex combo batch 156 — `tabulated-list` / `tabulated-list-mode` /
//! `derived-mode` / `text-property-list` patterns.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx156_tabulated_list_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tabulated-list)
      (list (fboundp 'tabulated-list-mode)
            (fboundp 'tabulated-list-print)
            (boundp 'tabulated-list-format)
            (boundp 'tabulated-list-entries)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_derived_mode_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'derived)
      (define-derived-mode neo-cx156-test-mode fundamental-mode
        "CX156"
        "docstring"
        (setq font-lock-defaults nil))
      (list (commandp 'neo-cx156-test-mode)
            (fboundp 'neo-cx156-test-mode)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_derived_mode_inherits_syntax_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-derived-mode neo-cx156-inherit-mode prog-mode
        "CX156-Inherit"
        "docstring")
      (with-temp-buffer
        (neo-cx156-inherit-mode)
        (list (eq major-mode 'neo-cx156-inherit-mode)
              (eq (derived-mode-p 'neo-cx156-inherit-mode 'prog-mode) t)
              (eq (derived-mode-p 'neo-cx156-inherit-mode 'text-mode) nil))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_text_property_list_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function text-property-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefg")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 4 6 'face 'italic)
  (let ((plist (text-property-list 1 7 (current-buffer))))
    (list (consp plist)
          (plist-member plist 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_add_text_properties_idempotent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((face bold) (face bold) (weight heavy face bold) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 4 '(face bold))
  (let ((first-add (text-properties-at 1)))
    (add-text-properties 1 4 '(face bold))
    (let ((second-add (text-properties-at 1)))
      (add-text-properties 1 4 '(weight heavy))
      (let ((third-add (text-properties-at 1)))
        (list first-add second-add third-add
              (equal first-add second-add))))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_set_text_property_single_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold bold italic italic nil 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 3 5 'face 'italic)
  (list (get-text-property 1 'face)
        (get-text-property 2 'face)
        (get-text-property 3 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face)
        (next-single-property-change 1 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx156_tabulated_list_format_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t t (\"Name\" 30 t) (\"Value\" 10 nil :right-align t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((fmt [("Name" 30 t)
                ("Value" 10 nil :right-align t)
                ("Tags" 0 nil)]))
      (list (vectorp fmt)
            (= (length fmt) 3)
            (aref fmt 0)
            (aref fmt 1)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_derived_mode_with_abbrev_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-derived-mode neo-cx156-abbrev-mode fundamental-mode
        "CX156-Abbrev"
        "docstring"
        (setq local-abbrev-table
              (make-abbrev-table)))
      (with-temp-buffer
        (neo-cx156-abbrev-mode)
        (list (eq major-mode 'neo-cx156-abbrev-mode)
              (consp local-abbrev-table))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_derived_mode_with_after_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (:fired))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (hook-fired)
      (define-derived-mode neo-cx156-hook-mode fundamental-mode
        "CX156-Hook"
        "docstring")
      (add-hook 'neo-cx156-hook-mode-hook (lambda () (push :fired hook-fired)))
      (with-temp-buffer
        (neo-cx156-hook-mode)
        (prog1 (list (eq major-mode 'neo-cx156-hook-mode)
                     (nreverse hook-fired))
          (remove-hook 'neo-cx156-hook-mode-hook (lambda () (push :fired hook-fired))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx156_text_property_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Tabulated/derived mega test buffer content")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 12 18 'face 'italic)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 18)))
        (overlay-put ov 'face 'region)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 25)
        (let ((state (list (text-property-list 1 20 (current-buffer))
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
