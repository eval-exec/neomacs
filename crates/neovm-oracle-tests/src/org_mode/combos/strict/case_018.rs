//! Combo-strict-18 — targeted divergence probes: org-table-eval-
//! formula, org-habit graph, org-element-type on all elements,
//! org-pcomplete specific completions, org-footer-goto-def,
//! org-babel-calc integration, org-timestamp-change with units.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_table_eval_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 5 | 3 |   |\n")
      (goto-char (point-min))
      (forward-char 6)  ;; on third column
      ;; org-table-eval-formula
      (condition-case nil
          (let ((result (org-table-eval-formula nil "$1*$2" nil nil nil)))
            (list :result result :buffer-after (buffer-string)))
        (error (list :eval-error t)))))))"##,
        expect,
    );
}

#[test]
fn strict_element_type_on_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (list
   ;; org-element-type on symbols
   (list :bold (org-element-type 'bold))
   (list :headline (org-element-type 'headline))
   (list :org-data (org-element-type 'org-data))
   (list :nil (org-element-type nil))
   ;; org-element-type on actual element
   (list :bold-elem
         (org-element-type
          (org-element-create 'bold nil "test")))
   )))"##,
        expect,
    );
}

#[test]
fn strict_footnote_goto_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Ref[fn:1]\n[fn:1] The definition.\n")
      (let ((r '()))
        (push (list :goto-fbound (fboundp 'org-footnote-goto-definition)) r)
        ;; goto definition from reference
        (goto-char (point-min))
        (search-forward "[fn:1]") (backward-char 2)
        (condition-case nil
            (progn (org-footnote-goto-definition "1")
                   (push (list :point-after-goto (point)) r))
          (error (push (list :goto-error t) r)))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_calc_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ob-calc-loaded t :ob-calc-execute-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (condition-case nil (require 'ob-calc) (error nil))
  (list
   :ob-calc-loaded (featurep 'ob-calc)
   :ob-calc-execute-fbound (fboundp 'org-babel-execute:calc)
   ))"##,
        expect,
    );
}

#[test]
fn strict_timestamp_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:change-fbound t) (:after-day \"<2024-06-16 Sun>\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2024-06-15 Sat>\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-timestamp-change
    (push (list :change-fbound (fboundp 'org-timestamp-change)) r)
    ;; change by 1 day
    (condition-case nil
        (progn (org-timestamp-change 1 'day)
               (push (list :after-day (buffer-string)) r))
      (error (push (list :change-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn strict_org_export_dictionary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:dictionary-fbound t :dictionary-keys (\"%e %n: %c\" \"Author\" \"Continued from previous page\" \"Continued on next page\" \"Created\" \"Date\" \"Equation\" \"Figure\" \"Figure %d:\" \"Footnotes\" \"List of Listings\" \"List of Tables\" \"Listing\" \"Listing %d:\" \"References\" \"See figure %s\" \"See listing %s\" \"See section %s\" \"See table %s\" \"Table\" \"Table %d:\" \"Table of Contents\" \"Unknown reference\") :translations-available t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   :dictionary-fbound (boundp 'org-export-dictionary)
   :dictionary-keys (when (boundp 'org-export-dictionary)
                      (mapcar #'car org-export-dictionary))
   :translations-available (when (boundp 'org-export-dictionary)
                             (> (length org-export-dictionary) 0))
   ))"##,
        expect,
    );
}

#[test]
fn strict_habit_insert_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:graph-fbound t :parse-fbound t :habit-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-habit)
  (list
   :graph-fbound (fboundp 'org-habit-insert-consistency-graphs)
   :parse-fbound (fboundp 'org-habit-parse-todo)
   :habit-fbound (fboundp 'org-is-habit-p)
   ))"##,
        expect,
    );
}

#[test]
fn strict_org_table_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:header-line-fbound t :toggle-coordinate-fbound t :toggle-formula-debugger-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   :header-line-fbound (fboundp 'org-table-header-line-mode)
   :toggle-coordinate-fbound (fboundp 'org-table-toggle-coordinate-overlays)
   :toggle-formula-debugger-fbound (fboundp 'org-table-toggle-formula-debugger)
   ))"##,
        expect,
    );
}

#[test]
fn strict_refile_get_location() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:get-location-fbound t :refile-fbound t :targets-fbound t :target-verify-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-refile)
  (list
   :get-location-fbound (fboundp 'org-refile-get-location)
   :refile-fbound (fboundp 'org-refile)
   :targets-fbound (fboundp 'org-refile-get-targets)
   :target-verify-fbound (boundp 'org-refile-target-verify-function)
   ))"##,
        expect,
    );
}

#[test]
fn strict_org_src_fontify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:fontify-block-fbound t :fontify-natively-bound nil :edit-src-fbound t :exit-src-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-src)
  (list
   :fontify-block-fbound (fboundp 'org-src-font-lock-fontify-block)
   :fontify-natively-bound (boundp 'org-src-fontify-natively)
   :edit-src-fbound (fboundp 'org-edit-src-code)
   :exit-src-fbound (fboundp 'org-edit-src-exit)
   ))"##,
        expect,
    );
}
