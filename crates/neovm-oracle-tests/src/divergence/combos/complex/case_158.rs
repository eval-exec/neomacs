//! Complex combo batch 158 — `exwm` / `tab-bar` / `character-fold` /
//! `latin1-display` / `disp-table` display tables with per-buffer
//! overrides.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx158_exwm_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'exwm)
          (fboundp 'exwm-init)
          (boundp 'exwm-workspace-number))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_char_fold_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'char-fold-symmetric)
      (boundp 'search-default-mode)
      (fboundp 'char-fold-to-regexp))
"##,
        expect,
    );
}

#[test]
fn div_cx158_char_fold_search_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "café naïve résumé")
      (let ((case-fold-search t))
        (goto-char 1)
        (list (search-forward "cafe" nil t)
              (search-forward "naive" nil t)
              (search-forward "resume" nil t))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_latin1_display_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'latin1-disp)
      (list (fboundp 'latin1-display)
            (boundp 'latin1-display-cache)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_disp_table_buffer_local_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([88] [89] nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf-a (get-buffer-create " *neo-cx158-a*"))
          (buf-b (get-buffer-create " *neo-cx158-b*"))
          (dt-a (make-display-table))
          (dt-b (make-display-table)))
      (aset dt-a ?A [?X])
      (aset dt-b ?A [?Y])
      (with-current-buffer buf-a (setq buffer-display-table dt-a))
      (with-current-buffer buf-b (setq buffer-display-table dt-b))
      (let ((got-a (with-current-buffer buf-a (aref buffer-display-table ?A)))
            (got-b (with-current-buffer buf-b (aref buffer-display-table ?A))))
        (kill-buffer buf-a)
        (kill-buffer buf-b)
        (list got-a got-b (eq got-a got-b))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_disp_table_standard_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'standard-display-underline)
          (fboundp 'standard-display-glyph)
          (boundp 'glyph-table))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_window_display_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (fboundp 'set-window-display-table)
        (window-display-table win)))
"##,
        expect,
    );
}

#[test]
fn div_cx158_buffer_display_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (list (boundp 'buffer-display-table)
        buffer-display-table))
"##,
        expect,
    );
}

#[test]
fn div_cx158_char_fold_include_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (char-fold-to-regexp nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((search-default-mode 'char-fold-to-regexp))
      (list search-default-mode
            (fboundp 'char-fold-make-table)
            (boundp 'char-fold-table)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx158_standard_display_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function standard-display-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dt (standard-display-table)))
  (list (or (null dt) (char-table-p dt))
        (fboundp 'standard-display-table)))
"##,
        expect,
    );
}

#[test]
fn div_cx158_disp_table_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((dt (make-display-table)))
      (aset dt ?X [?Y])
      (with-temp-buffer
        (buffer-enable-undo)
        (setq buffer-display-table dt)
        (insert "XXX YYY ZZZ content")
        (put-text-property 1 4 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (aref buffer-display-table ?X)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
