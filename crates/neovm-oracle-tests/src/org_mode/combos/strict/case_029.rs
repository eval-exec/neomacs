//! combo_strict_29.rs + strong 93/94 — exhaustive surface probes
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_org_babel_execute_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:es-fbound t :eb-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core)
 (list :es-fbound (fboundp 'org-babel-execute-subtree) :eb-fbound (fboundp 'org-babel-execute-buffer)))"##,
        expect,
    );
}
#[test]
fn strict_org_export_backend_name_from_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument org-export-backend nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox) (list
 :ascii-name (org-export-backend-name (assq 'ascii org-export-registered-backends))
 :html-name (org-export-backend-name (assq 'html org-export-registered-backends))
 :latex-name (org-export-backend-name (assq 'latex org-export-registered-backends))))"##,
        expect,
    );
}
#[test]
fn strict_org_element_parent_of_root() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'org-element)
 (with-temp-buffer (org-mode) (insert "* H\n") (let* ((t (org-element-parse-buffer))
  (h (car (org-element-map t 'headline #'identity))))
  (list :h-level (org-element-property :level h) :h-parent (org-element-type (org-element-property :parent h))
   :root-parent (org-element-property :parent t)))))"##,
        expect,
    );
}
#[test]
fn strict_org_entity_unicode_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (with-temp-buffer (org-mode)
 (insert "\\alpha \\beta \\gamma\n") (goto-char (point-min))
 (let ((t (org-element-parse-buffer))) (list
  :ent-count (length (org-element-map t 'entity #'identity))
  :ent-names (mapcar (lambda (e) (org-element-property :name e)) (org-element-map t 'entity #'identity))))))"##,
        expect,
    );
}
#[test]
fn strict_org_babel_hide_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:hide-result-fbound t :remove-result-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :hide-result-fbound (fboundp 'org-babel-hide-result-toggle) :remove-result-fbound (fboundp 'org-babel-remove-result)))"##,
        expect,
    );
}
#[test]
fn strict_org_cycle_local_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:after-cycle-C-invis nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody.\n* C\n") (let ((r '())) (goto-char (point-min))
  (search-forward "* C") (beginning-of-line)
  (condition-case nil (org-cycle) (error nil))
  (push (list :after-cycle-C-invis (get-char-property (point) 'invisible)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_prepare_search_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:psb-fbound nil :psb-day-fbound nil :buf-list t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
 (list :psb-fbound (fboundp 'org-prepare-agenda-buffers) :psb-day-fbound (fboundp 'org-prepare-agenda-buffers)
  :buf-list (when (fboundp 'org-agenda-files) (fboundp 'org-agenda-files))))"##,
        expect,
    );
}
#[test]
fn strict_org_macro_case_sensitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+MACRO: test MyTest\n{{{test}}} and {{{Test}}} and {{{TEST}}}\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)) (i (substring-no-properties (org-element-interpret-data t))))
  (push (list :has-MyTest (string-match-p "MyTest" i)) r)
  (push (list :still-has-Test (string-match-p "{{{Test}}}" i)) r)
  (push (list :still-has-TEST (string-match-p "{{{TEST}}}" i)) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_table_relative_row_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:to-lisp ((#(\"a\" 0 1 (face org-table)) #(\"a\" 0 1 (face org-table))) (#(\"1\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table))) (#(\"4\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a |  b |\n| 1 |  2 |\n| 3 |  4 |\n|   |    |\n")
 (insert "#+TBLFM: @>$1=vsum(@2..@-1)::$2=$1+0\n")
 (let ((r '())) (goto-char (point-min)) (org-table-recalculate t) (org-table-align)
  (push (list :to-lisp (org-table-to-lisp)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn strict_org_cite_style_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:styles nil :default-style nil :export-bibliography nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'oc) (list
 :styles (when (boundp 'org-cite-supported-styles) org-cite-supported-styles)
 :default-style (when (boundp 'org-cite-default-style) org-cite-default-style)
 :export-bibliography (when (boundp 'org-cite-export-bibliography) org-cite-export-bibliography)))"##,
        expect,
    );
}
