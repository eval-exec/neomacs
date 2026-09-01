//! combo_strict_30.rs + strong 95/96 — terminal probes
use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn strict_large_property_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:len 200 :first-char 120)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
  (let ((big (make-string 200 ?x))) (org-entry-put nil "BIG" big)
   (list :len (length (org-entry-get nil "BIG")) :first-char (string-to-char (org-entry-get nil "BIG"))))))"##,
        expect,
    );
}
#[test]
fn strict_property_with_colons_and_equals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:url \"https://a:b@c.com:8080/path?k=v&x=y\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (with-temp-buffer (org-mode) (insert "* H\n") (goto-char (point-min))
  (org-entry-put nil "URL" "https://a:b@c.com:8080/path?k=v&x=y")
  (list :url (org-entry-get nil "URL"))))"##,
        expect,
    );
}
#[test]
fn strict_babel_inline_src_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-emacs-lisp)
 (with-temp-buffer (org-mode) (let ((org-confirm-babel-evaluate nil))
  (insert "src_emacs-lisp{(+ 1 2)} ")
  (goto-char (point-min)) (search-forward "src_emacs-lisp")
  (condition-case nil (org-babel-execute-src-block) (error :exec-error)))))"##,
        expect,
    );
}
#[test]
fn strict_element_type_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:bold-eq-bold t :hl-eq-hl t :bold-ne-hl nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (list :bold-eq-bold (eq 'bold 'bold) :hl-eq-hl (eq 'headline 'headline) :bold-ne-hl (eq 'bold 'headline)))"##,
        expect,
    );
}
#[test]
fn strict_org_return_electric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:after \"* H\\n- item\\n\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n- item\n") (goto-char (point-min)) (search-forward "item") (end-of-line)
 (condition-case nil (org-return) (error nil)) (list :after (buffer-string)))"##,
        expect,
    );
}
#[test]
fn strict_org_metareturn_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:heads (\"H\" \"\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n") (goto-char (point-max)) (condition-case nil (org-insert-heading) (error nil))
 (list :heads (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
  (org-element-map (org-element-parse-buffer) 'headline #'identity))))"##,
        expect,
    );
}
#[test]
fn strict_org_table_move_column_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:after ((\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b | c |\n| 1 | 2 | 3 |\n") (goto-char (point-min))
 (forward-line 1) (forward-char 3) (condition-case nil (org-table-move-column 'left) (error nil))
 (list :after (org-table-to-lisp)))"##,
        expect,
    );
}
#[test]
fn strict_org_copy_visible_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody.\n** C\nBody.\n") (goto-char (point-min))
 (org-overview) (org-copy-visible) (list :fbound (fboundp 'org-copy-visible)))"##,
        expect,
    );
}
#[test]
fn strict_org_mark_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:marked nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\nBody.\n| a |\n| 1 |\n") (goto-char (point-min))
 (search-forward "| a |") (beginning-of-line)
 (condition-case nil (org-mark-element) (error nil)) (list :marked (region-active-p)))"##,
        expect,
    );
}
#[test]
fn strict_org_kill_line_in_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:after \"\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* TODO Task :work:\n") (goto-char (point-min))
 (condition-case nil (org-kill-line) (error nil))
 (list :after (buffer-string)))"##,
        expect,
    );
}
