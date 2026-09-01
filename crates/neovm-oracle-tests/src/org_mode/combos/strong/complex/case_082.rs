use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo82_babel_lob_ingest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ingest-fbound t :lob-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-lob) (list
 :ingest-fbound (fboundp 'org-babel-lob-ingest) :lob-fbound (fboundp 'org-babel-lob-execute)))"##,
        expect,
    );
}
#[test]
fn combo82_agenda_set_restriction_lock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:restrict-fbound t :remove-restrict-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :restrict-fbound (fboundp 'org-agenda-set-restriction-lock)
 :remove-restrict-fbound (fboundp 'org-agenda-remove-restriction-lock)))"##,
        expect,
    );
}
#[test]
fn combo82_org_back_to_heading_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading \"B\") (:back-to-B \"B\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody.\n*** C\n* D\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "Body.") (goto-char (match-beginning 0))
  (push (list :heading (org-get-heading t t t t)) r)
  (org-back-to-heading) (push (list :back-to-B (substring-no-properties (org-element-property :raw-value (org-element-at-point)))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo82_org_crypt_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:crypt-key-fbound t :crypt-disable-fbound t :encrypt-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-crypt) (list
 :crypt-key-fbound (boundp 'org-crypt-key) :crypt-disable-fbound (boundp 'org-crypt-disable-auto-save)
 :encrypt-fbound (fboundp 'org-encrypt-entry)))"##,
        expect,
    );
}
#[test]
fn combo82_babel_expand_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:expand-fbound t :expand-body-fbound nil :noweb-expand-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :expand-fbound (fboundp 'org-babel-expand-src-block)
 :expand-body-fbound (fboundp 'org-babel-expand-body:emacs-lisp)
 :noweb-expand-fbound (fboundp 'org-babel-expand-noweb-references)))"##,
        expect,
    );
}
#[test]
fn combo82_org_down_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:down-fbound t) (:up-fbound t) (:backward-fbound t) (:forward-fbound t) (:after-forward headline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n| a | b |\n| 1 | 2 |\n- item\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :down-fbound (fboundp 'org-down-element)) r)
  (push (list :up-fbound (fboundp 'org-up-element)) r)
  (push (list :backward-fbound (fboundp 'org-backward-element)) r)
  (push (list :forward-fbound (fboundp 'org-forward-element)) r)
  ;; move forward to table
  (condition-case nil (org-forward-element) (error nil))
  (push (list :after-forward (org-element-type (org-element-at-point))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo82_org_element_interpret_data_table_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:cell-type table-cell :row-type table-row :table-type table)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element) (list
 :cell-type (org-element-type (org-element-create 'table-cell nil "content"))
 :row-type (org-element-type (org-element-create 'table-row '(:type standard) (org-element-create 'table-cell nil "a")))
 :table-type (org-element-type (org-element-create 'table '(:type org)
   (org-element-create 'table-row '(:type standard) (org-element-create 'table-cell nil "x"))))))"##,
        expect,
    );
}
#[test]
fn combo82_org_export_ignore_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:has-A 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* A :ignore:\nBody.\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :has-A (and out (string-match-p "A" out))) r))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo82_property_inheritance_across_org_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:H-var \"y=2\") (:SH-var \"y=2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+PROPERTY: var x=1\n* H\n:PROPERTIES:\n:var+: y=2\n:END:\n** SH\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "* H") (beginning-of-line)
  (push (list :H-var (org-entry-get nil "var")) r)
  (search-forward "** SH") (beginning-of-line) (push (list :SH-var (org-entry-get nil "var" t)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo82_org_table_sum_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b | c |\n|---+---+---|\n| 1 | 2 |   |\n| 3 | 4 |   |\n")
 (insert "#+TBLFM: @>$1=vsum(@2$1..@-1$1)::$3=$1+$2\n")
 (let ((r '())) (goto-char (point-min)) (org-table-recalculate t) (org-table-align)
  (goto-char (point-min)) (forward-line 1) (push (list :row1-c (org-table-get "c" nil)) r)
  (forward-line) (push (list :row2-c (org-table-get "c" nil)) r) (nreverse r)))"##,
        expect,
    );
}
