use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_insert_structure_template_region_src_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((transient-mark-mode t))
      (org-mode)
      (insert "* Heading\n")
      (insert "(message \"*not a headline*\")\n")
      (insert ",#+already escaped\n")
      (goto-char (point-min))
      (forward-line 1)
      (push-mark (point) nil t)
      (goto-char (point-max))
      (org-insert-structure-template "src emacs-lisp")
      (let ((after-src (buffer-substring-no-properties
                        (point-min) (point-max))))
        (goto-char (point-max))
        (insert "Raw HTML\n")
        (push-mark (line-beginning-position) nil t)
        (goto-char (point-max))
        (org-insert-structure-template "EXPORT html")
        (list after-src
              (point)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_structure_template_menu_error_escape_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid structure type: nil\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((transient-mark-mode t)
          (warnings nil)
          (org-structure-template-alist
           '(("a" . "src emacs-lisp")
             ("aa" . "example")
             ("ab" . "comment")
             ("Q" . "QUOTE")
             ("old" "#+BEGIN_SRC ?\n#+END_SRC"))))
      (org-mode)
      (insert "* Templates\n")
      (insert "#+begin_src shell\n")
      (insert "echo already block\n")
      (insert "#+end_src\n")
      (insert "(message \"needs escaping\")\n")
      (insert "*not a headline inside code*\n")
      (insert "#+begin_example\n")
      (insert "example marker inside code\n")
      (insert "#+end_example\n")
      (let ((keys-ok (org--insert-structure-template-unique-keys
                      '("a" "aa" "ab" "abc" "b" "ba")))
            (keys-error
             (condition-case err
                 (org--insert-structure-template-unique-keys
                  '("aa" "aa"))
               (error (cons (car err) (cdr err)))))
            (invalid-error
             (condition-case err
                 (org-insert-structure-template nil)
               (error (cons (car err) (cdr err))))))
        (cl-letf (((symbol-function 'org-display-warning)
                   (lambda (message &rest args)
                     (push (list message args) warnings))))
          (org--check-org-structure-template-alist))
        (goto-char (point-min))
        (search-forward "(message")
        (push-mark (match-beginning 0) nil t)
        (goto-char (point-max))
        (cl-letf (((symbol-function 'org--insert-structure-template-mks)
                   (lambda () (cons "a" "src emacs-lisp :results output"))))
          (call-interactively 'org-insert-structure-template))
        (let ((after-src
               (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-max))
          (insert "\nComment <inside> region\n#+not keyword\n")
          (push-mark (save-excursion
                       (search-backward "Comment <inside>")
                       (point))
                     nil t)
          (goto-char (point-max))
          (cl-letf (((symbol-function 'org--insert-structure-template-mks)
                     (lambda () (cons "ab" "comment"))))
            (call-interactively 'org-insert-structure-template))
          (let ((after-comment
                 (buffer-substring-no-properties (point-min) (point-max))))
            (goto-char (point-max))
            (insert "\nRaw export body\n")
            (push-mark (line-beginning-position) nil t)
            (goto-char (point-max))
            (cl-letf (((symbol-function 'org--insert-structure-template-mks)
                       (lambda () (cons "Q" "EXPORT html"))))
              (call-interactively 'org-insert-structure-template))
            (let ((empty-error
                   (cl-letf (((symbol-function
                               'org--insert-structure-template-mks)
                              (lambda ()
                                (cons "\t"
                                      "Press TAB, RET or SPC to write block name")))
                             ((symbol-function 'read-string)
                              (lambda (&rest _) "")))
                     (condition-case err
                         (call-interactively 'org-insert-structure-template)
                       (error (cons (car err) (cdr err)))))))
              (list keys-ok
                    keys-error
                    invalid-error
                    (nreverse warnings)
                    after-src
                    after-comment
                    empty-error
                    (org-element-map
                        (org-element-parse-buffer)
                        '(headline src-block example-block comment-block
                          export-block)
                      (lambda (el)
                        (list (org-element-type el)
                              (org-element-property :language el)
                              (org-element-property :type el)
                              (org-element-property :value el)
                              (org-element-property :begin el)
                              (org-element-property :end el))))
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_tempo_custom_blocks_keywords_include_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"<L\" \"<Q\" \"<c\" \"<el\" \"<o\" \"<s\" \"<v\") ((\"<o\" . tempo-template-org-options) (\"<c\" . tempo-template-org-caption) (\"<L\" . tempo-template-org-latex) (\"<v\" . tempo-template-org-verse) (\"<el\" . tempo-template-org-src-emacs-lisp) (\"<Q\" . tempo-template-org-QUOTE) (\"<s\" . tempo-template-org-src) (\"<I\" . tempo-template-org-include)) \"#+begin_src emacs-lisp\\n(+ 1 2)\\n#+end_src\\n#+BEGIN_QUOTE\\nQuoted\\n\\n#+END_QUOTE\\n#+caption: A caption\\n#+include: \\\"snippet.org\\\" :lines \\\"1-1\\\"\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-tempo)
  (let* ((root (make-temp-file "org-tempo" t))
         (include-file (expand-file-name "snippet.org" root))
         (default-directory root)
         (org-structure-template-alist
          '(("s" . "src")
            ("Q" . "QUOTE")
            ("el" . "src emacs-lisp")
            ("v" . "verse")))
         (org-tempo-keywords-alist
          '(("L" . "latex")
            ("c" . "caption")
            ("o" . "options"))))
    (unwind-protect
        (progn
          (with-temp-file include-file (insert "* Included\n"))
          (with-temp-buffer
            (org-mode)
            (org-tempo-setup)
            (insert "<el")
            (org-tempo-complete-tag)
            (insert "(+ 1 2)")
            (goto-char (point-max))
            (insert "\n<Q")
            (org-tempo-complete-tag)
            (insert "Quoted\n")
            (goto-char (point-max))
            (insert "\n<c")
            (org-tempo-complete-tag)
            (insert "A caption")
            (goto-char (point-max))
            (insert "\n<I")
            (cl-letf (((symbol-function 'read-file-name)
                       (lambda (&rest _) include-file)))
              (org-tempo-complete-tag))
            (insert ":lines \"1-1\"")
            (list (sort (org-tempo--keys) #'string<)
                  org-tempo-tags
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_tempo_duplicate_update_include_abort_ast_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp (\"<m\" . tempo-template-org-macro))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-tempo)
  (let* ((root (make-temp-file "org-tempo-update" t))
         (include-file (expand-file-name "inc.org" root))
         (default-directory root)
         (org-structure-template-alist
          '(("x" . "src emacs-lisp")
            ("q" . "quote")
            ("Q" . "QUOTE")))
         (org-tempo-keywords-alist
          '(("k" . "keywords")
            ("m" . "macro"))))
    (unwind-protect
        (progn
          (with-temp-file include-file (insert "* Included\n"))
          (with-temp-buffer
            (org-mode)
            (org-tempo-setup)
            (insert "<x")
            (org-tempo-complete-tag)
            (insert "(+ 1 2)")
            (goto-char (point-max))
            (insert "\n<q")
            (org-tempo-complete-tag)
            (insert "lower quote")
            (goto-char (point-max))
            (insert "\n<Q")
            (org-tempo-complete-tag)
            (insert "upper quote")
            (let ((initial-tags
                   (mapcar (lambda (tag)
                             (list (car tag) (nth 2 tag)))
                           org-tempo-tags)))
              (setq org-structure-template-alist
                    '(("x" . "src shell")
                      ("e" . "example")))
              (setq org-tempo-keywords-alist
                    '(("k" . "caption")
                      ("z" . "latex")))
              (goto-char (point-max))
              (insert "\n<x")
              (org-tempo-complete-tag)
              (insert "echo updated")
              (goto-char (point-max))
              (insert "\n<e")
              (org-tempo-complete-tag)
              (insert "example body")
              (goto-char (point-max))
              (insert "\n<k")
              (org-tempo-complete-tag)
              (insert "Caption text")
              (goto-char (point-max))
              (insert "\n<I")
              (cl-letf (((symbol-function 'read-file-name)
                         (lambda (&rest _) (keyboard-quit))))
                (condition-case nil
                    (org-tempo-complete-tag)
                  (quit 'quit)))
              (let ((after-abort
                     (buffer-substring-no-properties
                      (line-beginning-position) (point))))
                (delete-region (line-beginning-position) (point))
                (insert "<I")
                (cl-letf (((symbol-function 'read-file-name)
                           (lambda (&rest _) include-file)))
                  (org-tempo-complete-tag))
                (insert ":minlevel 2")
                (list (sort (org-tempo--keys) #'string<)
                      initial-tags
                      (mapcar (lambda (tag)
                                (list (car tag) (nth 2 tag)))
                              org-tempo-tags)
                      after-abort
                      (org-element-map
                          (org-element-parse-buffer)
                          '(src-block quote-block example-block keyword)
                        (lambda (e)
                          (list (org-element-type e)
                                (org-element-property :language e)
                                (org-element-property :key e)
                                (org-element-property :value e)
                                (org-element-property :begin e)
                                (org-element-property :end e))))
                      (buffer-substring-no-properties
                       (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_table_convert_transpose_move_copy_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "Name,Jan,Feb\nAlpha,1,2\nBeta,3,4\n")
    (org-table-convert-region (point-min) (point-max) ",")
    (org-table-align)
    (let ((after-convert
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "Jan")
      (org-table-insert-column)
      (org-table-blank-field)
      (insert "Q1")
      (goto-char (point-min))
      (search-forward "Alpha")
      (org-table-copy-down 1)
      (org-table-move-row-down)
      (goto-char (point-min))
      (search-forward "Feb")
      (org-table-move-column-left)
      (let ((after-mutations
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (org-table-transpose-table-at-point)
        (list after-convert
              after-mutations
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_structure_edit_special_export_context_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No special environment to edit here\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (require 'ox-html)
  (require 'ox-ascii)
  (with-temp-buffer
    (let ((transient-mark-mode t)
          (org-src-window-setup 'current-window)
          (org-edit-src-content-indentation 2)
          (org-src-preserve-indentation nil)
          (org-edit-fixed-width-region-mode 'fundamental-mode))
      (org-mode)
      (insert "#+TITLE: Structure Combo\n")
      (insert "* Blocks\n")
      (insert "(message \"one\")\n(message \"two\")\n")
      (push-mark (save-excursion
                   (goto-char (point-min))
                   (search-forward "(message \"one\")")
                   (match-beginning 0))
                 nil t)
      (goto-char (point-min))
      (search-forward "(message \"two\")")
      (org-insert-structure-template "src emacs-lisp :results value")
      (let ((after-src-wrap
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-max))
        (insert "\nRaw <b>html</b>\n")
        (push-mark (line-beginning-position) nil t)
        (goto-char (point-max))
        (org-insert-structure-template "EXPORT html")
        (goto-char (point-max))
        (insert "\nExample one\nExample two\n")
        (push-mark (save-excursion
                     (search-backward "Example one")
                     (point))
                   nil t)
        (goto-char (point-max))
        (org-insert-structure-template "example")
        (let ((after-all-wrap
               (buffer-substring-no-properties (point-min) (point-max)))
              block-moves edit-src-mode edit-src-before edit-export-mode
              edit-export-before contexts html ascii)
          (goto-char (point-min))
          (org-next-block 1)
          (push (list 'next1 (line-number-at-pos)
                      (org-element-type (org-element-at-point))
                      (org-in-src-block-p t))
                block-moves)
          (org-next-block 1)
          (push (list 'next2 (line-number-at-pos)
                      (org-element-type (org-element-at-point))
                      (org-in-block-p '("export" "src" "example")))
                block-moves)
          (org-next-block 1)
          (push (list 'next3 (line-number-at-pos)
                      (org-element-type (org-element-at-point)))
                block-moves)
          (org-previous-block 2)
          (push (list 'prev2 (line-number-at-pos)
                      (org-element-type (org-element-at-point)))
                block-moves)
          (goto-char (point-min))
          (search-forward "(message \"one\")")
          (org-edit-special)
          (setq edit-src-mode major-mode
                edit-src-before
                (buffer-substring-no-properties (point-min) (point-max)))
          (goto-char (point-max))
          (insert "\n(message \"three\")")
          (org-edit-src-exit)
          (goto-char (point-min))
          (search-forward "Raw <b>html</b>")
          (org-edit-special)
          (setq edit-export-mode major-mode
                edit-export-before
                (buffer-substring-no-properties (point-min) (point-max)))
          (goto-char (point-max))
          (insert "\n<i>added</i>")
          (org-edit-src-exit)
          (setq contexts
                (mapcar
                 (lambda (needle)
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward needle)
                     (list needle
                           (mapcar #'car (org-context))
                           (org-element-type (org-element-context))
                           (org-in-src-block-p t)
                           (org-in-block-p '("src" "export" "example")))))
                 '("(message \"three\")" "Raw <b>html</b>"
                   "<i>added</i>" "Example two")))
          (setq html (org-export-as 'html nil nil t '(:with-toc nil))
                ascii (org-export-as 'ascii nil nil t '(:with-toc nil)))
          (let ((tree (org-element-parse-buffer)))
            (list after-src-wrap
                  after-all-wrap
                  (nreverse block-moves)
                  edit-src-mode
                  edit-src-before
                  edit-export-mode
                  edit-export-before
                  contexts
                  (org-element-map tree
                      '(src-block export-block example-block)
                    (lambda (el)
                      (list (org-element-type el)
                            (org-element-property :language el)
                            (org-element-property :parameters el)
                            (org-element-property :begin el)
                            (org-element-property :end el)))))
                  (mapcar (lambda (needle)
                            (not (null
                                  (string-match-p needle html))))
                          '("<b>html</b>" "<i>added</i>"
                            "(message &quot;three&quot;)"))
                  (mapcar (lambda (needle)
                            (not (null
                                  (string-match-p needle ascii))))
                          '("Example one" "Example two"
                            "(message \"three\")"))
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_tempo_block_expand_edit_src_exit_writeback_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-tempo)
  (require 'org-src)
  (with-temp-buffer
    (org-mode)
    (insert "<s")
    (org-tempo-complete-tag)
    (let ((after-src (buffer-substring-no-properties
                      (point-min) (point-max))))
      ;; Edit src block content
      (goto-char (point-min))
      (search-forward "#+begin_src")
      (forward-line 1)
      (insert "(+ 1 2)\n(message \"hello\")\n")
      (let ((after-edit (buffer-substring-no-properties
                         (point-min) (point-max))))
        ;; Enter edit mode
        (goto-char (point-min))
        (search-forward "(+ 1 2)")
        (org-edit-src-code)
        (let ((edit-mode major-mode)
              (edit-buf (buffer-substring-no-properties
                         (point-min) (point-max))))
          ;; Modify and exit
          (erase-buffer)
          (insert "(+ 10 20)\n(message \"modified\")\n")
          (org-edit-src-exit)
          (let ((after-exit (buffer-substring-no-properties
                             (point-min) (point-max))))
            ;; Now expand quote block
            (goto-char (point-max))
            (insert "\n<q")
            (org-tempo-complete-tag)
            (insert "Quoted text.\n")
            (let ((after-quote (buffer-substring-no-properties
                                (point-min) (point-max))))
              ;; Expand example block
              (goto-char (point-max))
              (insert "\n<e")
              (org-tempo-complete-tag)
              (insert "Example text.\n")
              (let ((after-example (buffer-substring-no-properties
                                    (point-min) (point-max))))
                ;; Parse all blocks
                (let ((blocks
                       (org-element-map (org-element-parse-buffer)
                           '(src-block quote-block example-block)
                         (lambda (el)
                           (list (org-element-type el)
                                 (org-element-property :language el)
                                 (org-element-property :value el)
                                 (org-element-property :begin el))))))
                  (list after-src
                        after-edit
                        edit-mode
                        edit-buf
                        after-exit
                        after-quote
                        after-example
                        blocks)))))))))))"##,
        expect,
    );
}
