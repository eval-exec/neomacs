//! combo_strict_28.rs — config mutation probes: odd-levels-only,
//! hide-leading-stars, indent-mode, pretty-entities, sub-superscripts,
//! footnote-auto-label, table-automatic-realign, support-shift-select,
//! adapt-indentation, and log-into-drawer under various settings.
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_config_odd_levels_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-odd-levels-only t)) (insert "* L1\n** L2\n*** L3\n**** L4\n")
  (let* ((t (org-element-parse-buffer)) (hs (org-element-map t 'headline #'identity)))
  (list :reduced (mapcar (lambda (h) (org-reduced-level (org-element-property :level h))) hs)
   :raw (mapcar (lambda (h) (org-element-property :level h)) hs))))))"##,
        expect,
    );
}
#[test]
fn strict_config_hide_leading_stars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-hide-leading-stars t)) (insert "* A\n** B\n")
  (let* ((t (org-element-parse-buffer)) (hs (org-element-map t 'headline #'identity)))
  (list :count (length hs) :raw (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h))) hs))))))"##,
        expect,
    );
}
#[test]
fn strict_config_footnote_auto_label() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-footnote-auto-label 'confirm)) (insert "Text[fn::inline].\n")
  (let* ((t (org-element-parse-buffer)) (frs (org-element-map t 'footnote-reference #'identity)))
  (list :count (length frs) :labels (mapcar (lambda (fr) (org-element-property :label fr)) frs))))))"##,
        expect,
    );
}
#[test]
fn strict_config_table_automatic_realign() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:to-lisp ((#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (#(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-table-automatic-realign t)) (insert "| a | b |\n| 1 | 2 |\n")
  (goto-char (point-min)) (org-table-align) (list :to-lisp (org-table-to-lisp)))))"##,
        expect,
    );
}
#[test]
fn strict_config_log_into_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:logbooks 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'org-clock)
 (with-temp-buffer (org-mode) (let ((org-log-into-drawer t) (org-clock-persist nil))
  (insert "* Task\n") (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (list :logbooks (length (org-element-map (org-element-parse-buffer) 'drawer
   (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))))))"##,
        expect,
    );
}
#[test]
fn strict_config_adapt_indentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:col 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-adapt-indentation t)) (insert "* A\n  Body.\n")
  (goto-char (point-max)) (list :col (current-column)))))"##,
        expect,
    );
}
#[test]
fn strict_config_property_inheritance_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 116)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-use-property-inheritance '("COLOR"))) (insert "* Root\n:PROPERTIES:\n:COLOR: blue\n:SIZE: big\n:END:\n** Leaf\n")
  (goto-char (point-max)) (list :color (org-entry-get (point) "COLOR" t) :size (org-entry-get (point) "SIZE" t))))))"##,
        expect,
    );
}
#[test]
fn strict_config_pretty_entities_on_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-pretty-entities t)) (insert "\\alpha *\\beta*.\n")
  (let* ((t (org-element-parse-buffer)) (ents (org-element-map t 'entity #'identity))
   (bolds (org-element-map t 'bold #'identity)))
  (list :entities (length ents) :bolds (length bolds))))))"##,
        expect,
    );
}
#[test]
fn strict_config_sub_superscript_off() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (with-temp-buffer (org-mode)
 (let ((org-use-sub-superscripts nil)) (insert "a_b and x^{2}.\n")
  (let* ((t (org-element-parse-buffer)) (subs (org-element-map t 'subscript #'identity))
   (sups (org-element-map t 'superscript #'identity)))
  (list :sub-count (length subs) :sup-count (length sups))))))"##,
        expect,
    );
}
#[test]
fn strict_config_export_with_toc_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:toc-present nil :toc-absent t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ox-ascii)
 (with-temp-buffer (org-mode) (let ((org-export-show-temporary-export-buffer nil))
  (insert "* H\n** S\n")
  (let ((with-toc (let ((org-export-with-toc t)) (org-export-as 'ascii nil nil t)))
        (without-toc (let ((org-export-with-toc nil)) (org-export-as 'ascii nil nil t))))
  (list :toc-present (and with-toc (string-match-p "Table of Contents" with-toc))
   :toc-absent (and without-toc (not (string-match-p "Table of Contents" without-toc))))))))"##,
        expect,
    );
}
