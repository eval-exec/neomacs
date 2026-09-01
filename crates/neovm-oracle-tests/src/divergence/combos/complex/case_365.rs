//! Complex combo batch 365 — `abbrev` expansion ultimate: define-abbrev with
//! hooks/case-fixed/enable-function, pre-abbrev-expand-hook, abbrev-table-map,
//! dynamic-abbrev/hippie-expand availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx365_abbrev_define_with_hooks_and_case_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"neocx365-expanded\" neo)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "neo" "neocx365-expanded"
                     (lambda () nil))
      (list (abbrev-table-p table)
            (abbrev-expansion "neo" table)
            (abbrev-symbol "neo" table)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_case_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha-beta-gamma\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "abc" "alpha-beta-gamma" nil :case-fixed t)
      (list (abbrev-expansion "abc" table)
            (abbrev-expansion "ABC" table)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_enable_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"conditional\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table))
          (enabled nil))
      (define-abbrev table "cond" "conditional" nil
                     :enable-function (lambda () enabled))
      (list (abbrev-expansion "cond" table)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_pre_abbrev_expand_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (fired)
      (add-hook 'pre-abbrev-expand-hook (lambda () (push :fired fired)))
      (prog1 (list (boundp 'pre-abbrev-expand-hook)
                   (consp pre-abbrev-expand-hook))
        (remove-hook 'pre-abbrev-expand-hook (lambda () (push :fired fired)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_table_map_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ab\" \"cd\" \"ef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "ab" "alpha-beta" nil)
      (define-abbrev table "cd" "charlie-delta" nil)
      (define-abbrev table "ef" "echo-foxtrot" nil)
      (let (collected)
        (mapatoms (lambda (sym)
                    (when (abbrev-expansion (symbol-name sym) table)
                      (push (symbol-name sym) collected)))
                  table)
        (sort collected #'string<)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_global_abbrev_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'global-abbrev-table)
      (abbrev-table-p global-abbrev-table)
      (boundp 'abbrev-file-name)
      (boundp 'save-abbrevs))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'abbrev-mode)
      (fboundp 'define-abbrev-table)
      (fboundp 'quietly-read-abbrev-file)
      (boundp 'abbrev-all-caps)
      (boundp 'abbrev-mode-hook))
"##,
        expect,
    )
}

#[test]
fn div_cx365_dabbrev_hippie_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dabbrev)
      (require 'hippie-exp)
      (list (fboundp 'dabbrev-expand)
            (fboundp 'dabbrev-completion)
            (boundp 'dabbrev-limit)
            (boundp 'dabbrev-case-fold-search)
            (fboundp 'hippie-expand)
            (boundp 'hippie-expand-try-functions-list)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_count_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((table (make-abbrev-table)))
  (define-abbrev table "aa" "alpha" nil)
  (define-abbrev table "bb" "beta" nil)
  (define-abbrev table "cc" "gamma" nil)
  (let (count)
    (mapatoms (lambda (s) (when (abbrev-expansion (symbol-name s) table)
                            (cl-incf count))) table)
    count))
"##,
        expect,
    )
}

#[test]
fn div_cx365_abbrev_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "neo" "neocx365-expanded-mega" nil)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Abbrev mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (abbrev-table-p table)
                             (abbrev-expansion "neo" table)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen()
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
