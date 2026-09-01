use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo93_org_babel_with_header_args_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:eval-when-bound nil :resolve-reference-fbound nil :number-p-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :eval-when-bound (boundp 'org-babel-check-evaluate) :resolve-reference-fbound (fboundp 'org-babel-ref-resolve)
 :number-p-fbound (fboundp 'org-babel-number-p)))"##,
        expect,
    );
}
#[test]
fn combo93_org_export_headline_levels_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:headline-levels-bound t :default-levels 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox) (list
 :headline-levels-bound (boundp 'org-export-headline-levels) :default-levels (when (boundp 'org-export-headline-levels) org-export-headline-levels)))"##,
        expect,
    );
}
#[test]
fn combo93_org_agenda_inactive_timestamp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:inactive-fbound nil :skip-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :inactive-fbound (fboundp 'org-agenda-skip-timestamp-if-deadline-is-shown)
 :skip-fbound (fboundp 'org-agenda-skip-if)))"##,
        expect,
    );
}
#[test]
fn combo93_org_babel_check_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:noweb-fbound t :noweb-ref-fbound nil :strip-noweb-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :noweb-fbound (fboundp 'org-babel-noweb-p) :noweb-ref-fbound (fboundp 'org-babel-noweb-ref)
 :strip-noweb-fbound (fboundp 'org-babel-strip-noweb)))"##,
        expect,
    );
}
#[test]
fn combo93_org_cycle_plain_lists_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:vis-items 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-cycle-include-plain-lists 3)) (insert "* H\n- item\n  - sub\n")
  (let ((r '())) (goto-char (point-min)) (org-cycle)
   (push (list :vis-items (length (org-element-map (org-element-parse-buffer nil t) 'item #'identity))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo93_org_table_column_formula_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:to-lisp ((#(\"0\" 0 1 (face org-table)) #(\"y\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table))) (#(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table))) (#(\"0\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table))) (#(\"0\" 0 1 (face org-table)) \"\" #(\"0\" 0 1 (face org-table))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| x | y | z |\n| 1 | 2 |   |\n| 3 | 4 |   |\n|   |   |   |\n")
 (insert "#+TBLFM: $3=$1*$2::@>$3=vsum(@2..@-1)::$1=0\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (progn (org-table-recalculate t) (org-table-align)
  (push (list :to-lisp (org-table-to-lisp)) r)) (error (push :err r))) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo93_org_capture_clocked_items() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:clock-fbound nil :get-clock-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-capture) (list
 :clock-fbound (fboundp 'org-capture-put-clock) :get-clock-fbound (fboundp 'org-capture-get-clock)))"##,
        expect,
    );
}
#[test]
fn combo93_org_dblock_create_update_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:err (:dblock-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+BEGIN: columnview :hlines 1 :id local\n#+END:\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line)
  (condition-case nil (progn (org-dblock-update) (push (list :updated t) r)) (error (push :err r)))
  (push (list :dblock-count (length (org-element-map (org-element-parse-buffer) 'dynamic-block #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo93_org_babel_tangle_header_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:tangle-w-comment-fbound t :tangle-jump-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-tangle) (list
 :tangle-w-comment-fbound (fboundp 'org-babel-tangle-comment-links) :tangle-jump-fbound (fboundp 'org-babel-tangle-jump-to-org)))"##,
        expect,
    );
}
#[test]
fn combo93_org_persist_debug_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:index-fbound t :hash-fbound t :associated-bound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-persist) (list
 :index-fbound (boundp 'org-persist--index) :hash-fbound (boundp 'org-persist--report-time)
 :associated-bound (boundp 'org-persist--associated)))"##,
        expect,
    );
}
