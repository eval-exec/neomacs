use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo76_org_babel_check_evaluate_confirm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 3 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (require 'ob-core)
 (list :confirm-eval-fbound (fboundp 'org-babel-check-confirm-evaluate)
  :eval-when-compare-fbound (boundp 'org-babel-check-evaluate))))"##,
        expect,
    );
}
#[test]
fn combo76_element_create_target_and_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:target-type target) (:link-type link) (:link-path \"my-target\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (let* ((target (org-element-create 'target '(:value "my-target")))
        (link (org-element-create 'link '(:type "custom-id" :path "my-target" :raw-link "#my-target")))
        (r '()))
  (push (list :target-type (org-element-type target)) r)
  (push (list :link-type (org-element-type link)) r)
  (push (list :link-path (org-element-property :path link)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo76_org_macro_replacement_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+MACRO: a Alice\n#+MACRO: b {{{a}}}\n* {{{b}}} says hi.\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer))
   (i (substring-no-properties (org-element-interpret-data t))))
  (push (list :has-alice (string-match-p "Alice" i)) r)
  (push (list :no-braces (not (string-match-p "{{{" i))) r))
 (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo76_org_table_with_named_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:to-lisp ((\"!\" \"Name\" \"Age\") hline (\"\" \"Alice\" \"30\") (\"\" \"Bob\" \"25\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| ! | Name | Age |\n|---+------+-----|\n|   | Alice|   30|\n|   | Bob  |   25|\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :to-lisp (org-table-to-lisp)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo76_org_agenda_sort_user_defined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:sort-strategy-bound t :cmp-user-fbound t :entry-type-bound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :sort-strategy-bound (boundp 'org-agenda-sorting-strategy)
 :cmp-user-fbound (boundp 'org-agenda-cmp-user-defined)
 :entry-type-bound (boundp 'org-agenda-entry-types)
 ))"##,
        expect,
    );
}
#[test]
fn combo76_org_babel_with_header_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:default-header-bound t :default-header-keys (:session :results :exports :cache :noweb :hlines :tangle) :default-header-emacs nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :default-header-bound (boundp 'org-babel-default-header-args)
 :default-header-keys (when (boundp 'org-babel-default-header-args)
   (mapcar #'car org-babel-default-header-args))
 :default-header-emacs (when (boundp 'org-babel-default-header-args:emacs-lisp)
   (mapcar #'car org-babel-default-header-args:emacs-lisp))
 ))"##,
        expect,
    );
}
#[test]
fn combo76_org_cycle_content_optimization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:cycle-include-plain-lists-fbound t :cycle-separator-lines-fbound t :cycle-max-level-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :cycle-include-plain-lists-fbound (boundp 'org-cycle-include-plain-lists)
 :cycle-separator-lines-fbound (boundp 'org-cycle-separator-lines)
 :cycle-max-level-fbound (boundp 'org-cycle-max-level)
 ))"##,
        expect,
    );
}
#[test]
fn combo76_org_timestamp_with_delay_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (let ((ts (org-timestamp-from-string "<2024-01-15 Mon -3d>")))
 (list :type (org-element-property :type ts) :warning-type (org-element-property :warning-type ts)
  :warning-value (org-element-property :warning-value ts)
  :warning-unit (org-element-property :warning-unit ts))))"##,
        expect,
    );
}
#[test]
fn combo76_org_export_latex_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:class-fbound t :class-length 3 :default-class \"article\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-latex) (list
 :class-fbound (boundp 'org-latex-classes)
 :class-length (when (boundp 'org-latex-classes) (length org-latex-classes))
 :default-class (when (boundp 'org-latex-default-class) org-latex-default-class)
 ))"##,
        expect,
    );
}
#[test]
fn combo76_org_footnote_normalize_sort_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ref-labels (\"z\" \"a\" \"m\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "Z[fn:z] A[fn:a] M[fn:m]\n[fn:z] Z.\n[fn:a] A.\n[fn:m] M.\n")
 (let ((r '())) (goto-char (point-min))
  (condition-case nil (org-footnote-normalize 'sort) (error nil))
  (push (list :ref-labels (mapcar (lambda (fr) (org-element-property :label fr))
    (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
