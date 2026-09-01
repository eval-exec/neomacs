//! Combo-strict-12 oracle tests — error paths and edge-case
//! input handling: wrong argument types, nil/empty args,
//! malformed buffer content, timestamp edge cases, link escape
//! with null input, table malformed formulas, footnote malformed
//! references, export with missing backends, babel unsupported
//! languages, element parsing with degenerate input, and
//! org-table with formula referencing non-existent columns.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_wrong_arg_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 29 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (list
   ;; org-entry-get with number instead of string
   (condition-case e
       (let ((org-mode-hook nil))
         (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
           (org-entry-get nil 42)))
     (error (list :entry-get-bad-key-type (car e))))
   ;; org-entry-put with nil value
   (condition-case e
       (let ((org-mode-hook nil))
         (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
           (org-entry-put nil "KEY" nil)))
     (error (list :entry-put-nil-val (car e))))
   ;; org-element-property with non-existent key
   (condition-case e
       (let ((org-mode-hook nil))
         (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
           (org-element-property :nonexistent (org-element-at-point))))
     (error (list :bad-prop-key (car e))))
   ;; org-element-map with bad type
   (condition-case e
       (let ((org-mode-hook nil))
         (with-temp-buffer (org-mode) (insert "* H\n")
           (org-element-map (org-element-parse-buffer) nil #'identity)))
     (error (list :map-nil-type (car e))))
   )))"##,
        expect,
    );
}

#[test]
fn strict_timestamp_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; empty string
   (condition-case e (org-timestamp-from-string "")
     (error (list :empty-string-error (car e))))
   ;; nil
   (condition-case e (org-timestamp-from-string nil)
     (error (list :nil-error (car e))))
   ;; non-date string
   (condition-case e (org-timestamp-from-string "hello")
     (error (list :bad-string-error (car e))))
   ;; missing closing bracket
   (condition-case e (org-timestamp-from-string "<2024-01-01")
     (error (list :missing-bracket-error (car e))))
   ;; empty angle brackets
   (condition-case e (org-timestamp-from-string "<>")
     (error (list :empty-brackets-error (car e))))
   ;; valid simple
   (condition-case nil
       (let ((ts (org-timestamp-from-string "<2024-06-15 Sat>")))
         (list :valid (org-element-property :year-start ts)))
     (error :ok-error))
   )))"##,
        expect,
    );
}

#[test]
fn strict_link_escape_null_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; nil input
   (condition-case e (org-link-escape nil)
     (error (list :escape-nil (car e))))
   ;; empty string
   (org-link-escape "")
   ;; very long string with many specials
   (let ((long-str (make-string 100 ?x)))
     (condition-case nil
         (let ((escaped (org-link-escape long-str)))
           (> (length escaped) 0))
       (error :long-error)))
   ;; unicode
   (org-link-escape "αβγ 日本語")
   ;; already-escaped percent
   (org-link-escape "%20%20")
   )))"##,
        expect,
    );
}

#[test]
fn strict_malformed_table_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n| 1 | 2 |\n")
      ;; malformed formula: reference to non-existent column
      (insert "#+TBLFM: $99=$1+$2\n")
      (condition-case e
          (progn (goto-char (point-min))
                 (org-table-recalculate t)
                 :no-error)
        (error (list :bad-col (car e)))))))"##,
        expect,
    );
}

#[test]
fn strict_malformed_footnote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:] and [fn::missing close bracket\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (refs (org-element-map tree 'footnote-reference #'identity))
             (r '()))
        (push (list :ref-count (length refs)) r)
        (push (list :ref-labels (mapcar (lambda (fr) (org-element-property :label fr)) refs)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_export_nonexistent_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:error-type error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Test\n")
      (condition-case e
          (org-export-as 'nonexistent-backend nil nil t)
        (error (list :error-type (car e)))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_unsupported_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:error-type error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src nonexistent-lang\nsome code\n#+end_src\n")
      (goto-char (point-min))
      (search-forward "#+begin_src")
      (condition-case e
          (org-babel-execute-src-block)
        (error (list :error-type (car e)))))))"##,
        expect,
    );
}

#[test]
fn strict_element_degenerate_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      ;; just stars with no content
      (insert "***\n*****\n*\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity))
             (r '()))
        (push (list :count (length headlines)) r)
        (push (list :levels (mapcar (lambda (h) (org-element-property :level h)) headlines)) r)
        (push (list :values (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h))) headlines)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_table_formula_nonexistent_col_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :no-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a |\n| 1 |\n")
      (insert "#+TBLFM: $5=$1*2\n")
      (goto-char (point-min))
      (condition-case e
          (progn (org-table-recalculate t) :no-error)
        (error (list :error (car e)))))))"##,
        expect,
    );
}

#[test]
fn strict_org_store_link_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ol)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Plain text without heading.\n")
      (list
       ;; store link on plain text (no heading)
       (progn (goto-char (point-min))
              (condition-case e
                  (org-store-link nil)
                (error (list :plain-link (car e)))))
       ;; store link with prefix arg
       (condition-case e
           (org-store-link 1)
         (error (list :interactive-link (car e))))))))"##,
        expect,
    );
}

#[test]
fn strict_org_babel_var_reference_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:error-type error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      ;; reference to non-existent named block
      (insert "#+begin_src emacs-lisp :results value :var x=nonexistent\nx\n#+end_src\n")
      (goto-char (point-min))
      (search-forward "#+begin_src")
      (condition-case e
          (org-babel-execute-src-block)
        (error (list :error-type (car e)))))))"##,
        expect,
    );
}

#[test]
fn strict_org_export_include_nonexistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:include-error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+INCLUDE: \"/nonexistent/file/for/testing.org\"\n")
      (insert "* Test\n")
      (condition-case e
          (progn (goto-char (point-min))
                 (org-export-as 'ascii nil nil t)
                 :no-error)
        (error (list :include-error (car e)))))))"##,
        expect,
    );
}
