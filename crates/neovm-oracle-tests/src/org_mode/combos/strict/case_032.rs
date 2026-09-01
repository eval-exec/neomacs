use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_org_inactive_timestamp_with_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (let ((ts (org-timestamp-from-string "[2024-06-15 Sat 10:30]")))
  (list :type (org-element-property :type ts) :hour (org-element-property :hour-start ts)
   :minute (org-element-property :minute-start ts) :day (org-element-property :day-start ts))))"##,
        expect,
    );
}
#[test]
fn strict_org_italic_at_line_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "/not italic/ at start\n")
  (let* ((t (org-element-parse-buffer)) (its (org-element-map t 'italic #'identity)))
   (list :italic-count (length its)))))"##,
        expect,
    );
}
#[test]
fn strict_org_verbatim_with_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "=code with *stars* and /slashes/=\n")
  (let* ((t (org-element-parse-buffer)) (vbs (org-element-map t 'verbatim #'identity)))
   (list :count (length vbs) :value (when (car vbs) (substring-no-properties
    (buffer-substring-no-properties (org-element-property :begin (car vbs))
     (org-element-property :end (car vbs)))))))))"##,
        expect,
    );
}
#[test]
fn strict_org_strikethrough_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "+struck+\n")
  (let* ((t (org-element-parse-buffer)) (sts (org-element-map t 'strike-through #'identity)))
   (list :count (length sts)))))"##,
        expect,
    );
}
#[test]
fn strict_org_underline_then_normal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "_underlined_ then plain\n")
  (let* ((t (org-element-parse-buffer)) (uns (org-element-map t 'underline #'identity)))
   (list :count (length uns)))))"##,
        expect,
    );
}
#[test]
fn strict_org_horizontal_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "-----\n")
  (let* ((t (org-element-parse-buffer)) (hrs (org-element-map t 'horizontal-rule #'identity)))
   (list :count (length hrs)))))"##,
        expect,
    );
}
#[test]
fn strict_org_diary_sexp_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "%%(diary-anniversary 1 1 2000) Event\n")
  (let* ((t (org-element-parse-buffer)) (des (org-element-map t 'diary-sexp #'identity)))
   (list :count (length des) :value (when (car des) (org-element-property :value (car des)))))))"##,
        expect,
    );
}
#[test]
fn strict_org_table_hlines_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "| a | b |\n|---+---|\n| 1 | 2 |\n|---+---|\n| 3 | 4 |\n")
  (let* ((t (org-element-parse-buffer)) (hrs (org-element-map t 'table-row
   (lambda (r) (when (eq (org-element-property :type r) 'rule) r)))))
   (list :hline-count (length hrs) :data-rows (- (length (org-element-map t 'table-row #'identity)) (length hrs))))))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_named_src_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:found t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core)
 (with-temp-buffer (org-mode) (insert "#+name: target\n#+begin_src emacs-lisp\n1\n#+end_src\n")
  (goto-char (point-min)) (condition-case nil (org-babel-goto-named-src-block "target")
   (error :not-found)) (list :found (looking-at "#\\+begin_src"))))"##,
        expect,
    );
}
#[test]
fn strict_org_macro_templates_default_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:templates-bound t :templates-count 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode)
  (org-macro-initialize-templates)
  (list :templates-bound (boundp 'org-macro-templates)
   :templates-count (when (boundp 'org-macro-templates) (length org-macro-templates)))))"##,
        expect,
    );
}
