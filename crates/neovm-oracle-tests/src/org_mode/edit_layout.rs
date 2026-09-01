use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_indent_region_drawer_list_block_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (let ((org-adapt-indentation t))
      (org-mode)
      (insert "* Parent\n")
      (insert ":PROPERTIES:\n:Effort: 1:00\n:END:\n")
      (insert "Paragraph under parent.\n")
      (insert "- item one\n  continuation\n- item two\n")
      (insert "#+begin_quote\nquoted\n#+end_quote\n")
      (insert "** Child\nBody\n")
      (org-indent-region (point-min) (point-max))
      (goto-char (point-min))
      (search-forward "Effort")
      (org-indent-line)
      (goto-char (point-min))
      (search-forward "item two")
      (beginning-of-line)
      (org-indent-item)
      (let ((tree (org-element-parse-buffer)))
        (list
         (buffer-substring-no-properties (point-min) (point-max))
         (org-element-map tree '(headline plain-list quote-block property-drawer)
           (lambda (e)
             (list (org-element-type e)
                   (org-element-property :begin e)
                   (org-element-property :end e))))))))"##,
        expect,
    );
}

#[test]
fn org_fill_paragraph_item_timestamp_macro_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((fill-column 42)
          (org-adapt-indentation nil))
      (org-mode)
      (insert "* Fill\n")
      (insert "Long paragraph before <2026-05-27 Wed 09:00> with {{{macro(arg)}}} and enough words to wrap across several lines.\n\n")
      (insert "- item with a timestamp <2026-05-28 Thu> and enough trailing words to wrap while staying inside the list item.\n")
      (insert "#+MACRO: macro value-$1\n")
      (goto-char (point-min))
      (search-forward "Long paragraph")
      (org-fill-paragraph)
      (search-forward "item with")
      (org-fill-paragraph)
      (list
       (buffer-substring-no-properties (point-min) (point-max))
       (org-element-map (org-element-parse-buffer) '(paragraph item keyword timestamp macro)
         (lambda (e)
           (list (org-element-type e)
                 (org-element-property :begin e)
                 (org-element-property :end e)
                 (org-element-property :key e)
                 (org-element-property :value e)
                 (org-element-property :name e)))))))"##,
        expect,
    );
}

#[test]
fn org_comment_uncomment_heading_block_region_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Comments\n")
    (insert "* Alpha\n")
    (insert "Body line one.\nBody line two.\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "* Beta\nBody beta.\n")
    (goto-char (point-min))
    (search-forward "Body line one")
    (let ((beg (line-beginning-position))
          (end (progn (forward-line 2) (point))))
      (org-comment-or-uncomment-region beg end)
      (let ((after-comment
             (buffer-substring-no-properties (point-min) (point-max))))
        (org-comment-or-uncomment-region beg end)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-insert-comment)
        (goto-char (point-min))
        (search-forward "* Beta")
        (beginning-of-line)
        (org-toggle-comment)
        (list after-comment
              (buffer-substring-no-properties (point-min) (point-max))
              (org-element-map (org-element-parse-buffer) '(comment comment-block headline src-block)
                (lambda (e)
                  (list (org-element-type e)
                        (org-element-property :begin e)
                        (org-element-property :end e)
                        (org-element-property :raw-value e))))))))"##,
        expect,
    );
}
