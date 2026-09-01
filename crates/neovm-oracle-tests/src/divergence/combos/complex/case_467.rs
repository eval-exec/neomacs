/// Batch 467: final deep edge probes - fill-column, auto-fill, dabbrev, hippie.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx467_fill_column_auto_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a long line that should auto-fill at column 30 for testing\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (text-mode)
  (setq fill-column 30)
  (auto-fill-mode 1)
  (insert "This is a long line that should auto-fill at column 30 for testing")
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx467_dabbrev_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'dabbrev)
  (list (boundp 'dabbrev-case-fold-search)
        (fboundp 'dabbrev-expand)))"##,
        expect,
    );
}

#[test]
fn div_cx467_hippie_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'hippie-exp)
  (list (boundp 'hippie-expand-try-functions-list)
        (fboundp 'he-substitute-string)))"##,
        expect,
    );
}

#[test]
fn div_cx467_isearch_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'isearch)
  (list (boundp 'isearch-mode-map)
        (fboundp 'isearch-forward)
        (fboundp 'isearch-backward)))"##,
        expect,
    );
}

#[test]
fn div_cx467_query_replace_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'replace)
  (list (boundp 'query-replace-map)
        (fboundp 'query-replace)
        (fboundp 'query-replace-regexp)))"##,
        expect,
    );
}

#[test]
fn div_cx467_shell_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'shell)
  (list (boundp 'shell-completion-fignore)
        (fboundp 'shell-dynamic-complete-command)))"##,
        expect,
    );
}

#[test]
fn div_cx467_completion_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'completion)
  (list (boundp 'completion-at-point-functions)
        (fboundp 'completion-at-point)))"##,
        expect,
    );
}

#[test]
fn div_cx467_minibuffer_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'minibuffer-history)
      (boundp 'minibuffer-history-variable)
      (listp minibuffer-history))"##,
        expect,
    );
}

#[test]
fn div_cx467_face_spec_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-spec-set 'bold '((t (:weight bold))) nil)
      (face-attribute 'bold :weight nil 'default))"##,
        expect,
    );
}

#[test]
fn div_cx467_read_buffer_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r##"(fboundp 'read-buffer)"##, expect);
}

#[test]
fn div_cx467_image_type_avail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (image-type-available-p 'png)
      (image-type-available-p 'jpeg)
      (image-type-available-p 'xpm))"##,
        expect,
    );
}

#[test]
fn div_cx467_doc_view_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'doc-view)
  (list (fboundp 'doc-view-mode)
        (fboundp 'doc-view-toggle-display)))"##,
        expect,
    );
}

#[test]
fn div_cx467_print_help_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'help-mode)
  (condition-case e
      (with-temp-buffer
        (help-mode)
        (print-help-return-message))
    (error (car e))))"##,
        expect,
    );
}

#[test]
fn div_cx467_display_battery_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'battery)
  (list (boundp 'display-battery-mode)
        (fboundp 'battery-status-function)))"##,
        expect,
    );
}

#[test]
fn div_cx467_abbrev_inverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'abbrev)
  (list (fboundp 'inverse-add-mode-abbrev)
        (fboundp 'inverse-add-global-abbrev)))"##,
        expect,
    );
}
