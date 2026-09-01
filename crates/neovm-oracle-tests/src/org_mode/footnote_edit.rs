use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_footnote_renumber_delete_sort_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Don’t know which footnote to remove\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Notes\n")
    (insert "B ref[fn:7], A ref[fn:3], B again[fn:7].\n\n")
    (insert "[fn:7] Bee definition\n")
    (insert "[fn:3] Aye definition\n")
    (insert "[fn:9] Unused definition\n")
    (org-footnote-renumber-fn:N)
    (let ((after-renumber
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-footnote-sort)
      (let ((after-sort
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "[fn:1]")
        (let ((deleted (org-footnote-delete)))
          (list after-renumber
                after-sort
                deleted
                (org-footnote-all-labels)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_local_inline_export_edit_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (require 'ox-ascii)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Footnote Combo\n")
    (insert "* Chapter One\n")
    (insert "Alpha[fn:a] inline[fn::Inline A] repeat[fn:a].\n")
    (insert "** Local Notes\n")
    (insert "[fn:a] A definition\n")
    (insert "[fn:orphan] Orphan definition\n")
    (insert "* Chapter Two\n")
    (insert "Beta[fn:b] inline[fn::Inline B with *bold*].\n")
    (insert "[fn:b] Bee definition\n")
    (insert "* Tail\n")
    (let ((org-footnote-section nil)
          (org-footnote-fill-after-inline-note-extraction nil)
          (org-export-with-toc nil))
      (let ((before
             (list (org-footnote-all-labels)
                   (org-footnote--collect-references 'anonymous)
                   (org-footnote--collect-definitions))))
        (org-footnote-normalize)
        (let ((after-normalize
               (buffer-substring-no-properties (point-min) (point-max)))
              (labels-normalized (org-footnote-all-labels))
              (defs-normalized (org-footnote--collect-definitions)))
          (goto-char (point-min))
          (search-forward "repeat")
          (let ((deleted-a (org-footnote-delete "a"))
                (after-delete-a nil))
            (setq after-delete-a
                  (buffer-substring-no-properties (point-min) (point-max)))
            (goto-char (point-min))
            (search-forward "Chapter Two")
            (search-forward "Beta")
            (let ((org-footnote-auto-label 'plain)
                  (org-footnote-define-inline nil)
                  (org-footnote-auto-adjust 'renumber))
              (org-footnote-new)
              (insert "Added note")
              (org-footnote-auto-adjust-maybe))
            (org-footnote-sort)
            (let* ((tree (org-element-parse-buffer))
                   (footnotes
                    (org-element-map tree 'footnote-reference
                      (lambda (fn)
                        (list (org-element-property :label fn)
                              (org-element-property :type fn)
                              (org-element-property :begin fn)
                              (org-element-property :end fn)))))
                   (paras
                    (org-element-map tree 'paragraph
                      (lambda (p)
                        (buffer-substring-no-properties
                         (org-element-property :begin p)
                         (org-element-property :end p)))))
                   (ascii
                    (org-export-string-as
                     (buffer-substring-no-properties
                      (point-min) (point-max))
                     'ascii t '(:with-toc nil))))
              (list before
                    labels-normalized
                    defs-normalized
                    after-normalize
                    deleted-a
                    after-delete-a
                    (org-footnote-all-labels)
                    (org-footnote--collect-references 'anonymous)
                    (org-footnote--collect-definitions)
                    footnotes
                    paras
                    ascii
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_inline_normalize_section_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"2\" \"1\") \"* Alpha\\nText [fn:1] and named [fn:2].\\n\\n* Footnotes\\n\\n[fn:1] Inline *bold* note\\n\\n[fn:2] Named note\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "Text [fn::Inline *bold* note] and named [fn:name].\n")
    (insert "* Footnotes\n")
    (insert "[fn:name] Named note\n")
    (let ((org-footnote-section "Footnotes")
          (org-footnote-fill-after-inline-note-extraction nil))
      (org-footnote-normalize)
      (list (org-footnote-all-labels)
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_reference_definition_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* One\n")
    (insert "First [fn:a] and second [fn:b].\n")
    (insert "* Footnotes\n")
    (insert "[fn:b] Bee\n")
    (insert "[fn:a] Aye\n")
    (goto-char (point-min))
    (search-forward "[fn:a]")
    (let ((ref-a (org-footnote-at-reference-p)))
      (org-footnote-goto-definition "a")
      (let ((def-a (list (line-number-at-pos)
                         (org-footnote-at-definition-p))))
        (org-footnote-goto-previous-reference "a")
        (let ((back-a (list (line-number-at-pos)
                            (org-footnote-at-reference-p))))
          (goto-char (point-max))
          (let ((pos (org-footnote-create-definition "c")))
            (list ref-a
                  def-a
                  back-a
                  pos
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_auto_label_inline_adjust_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"2\" \"1\") ((\"1\" #<marker in no buffer> t nil) (\"2\" #<marker in no buffer> t nil)) ((\"2\" . \"[fn:2] Definition B\") (\"1\" . \"[fn:1] Inline A\")) \"* Body\\nAlpha[fn:1] sentence. Beta[fn:2] sentence.\\n\\n* Footnotes\\n\\n[fn:1] Inline A\\n\\n[fn:2] Definition B\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Body\n")
    (insert "Alpha sentence. Beta sentence.\n")
    (goto-char (point-min))
    (search-forward "Alpha")
    (let ((org-footnote-auto-label t)
          (org-footnote-define-inline t)
          (org-footnote-auto-adjust 'sort)
          (org-footnote-fill-after-inline-note-extraction nil))
      (org-footnote-new)
      (insert "Inline A")
      (goto-char (point-min))
      (search-forward "Beta")
      (let ((org-footnote-define-inline nil))
        (org-footnote-new)
        (insert "Definition B"))
      (org-footnote-normalize)
      (list (org-footnote-all-labels)
            (org-footnote--collect-references 'anonymous)
            (org-footnote--collect-definitions)
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_delete_label_references_definitions_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Text\n")
    (insert "A[fn:keep] B[fn:drop] C[fn:drop] D[fn::Anon]\n\n")
    (insert "[fn:keep] Keep definition\n")
    (insert "[fn:drop] Drop definition 1\n")
    (insert "[fn:drop] Drop definition 2\n")
    (let ((refs (org-footnote-delete-references "drop"))
          (defs (org-footnote-delete-definitions "drop")))
      (goto-char (point-min))
      (search-forward "Anon")
      (let ((anon-deleted (org-footnote-delete)))
        (list refs
              defs
              anon-deleted
              (org-footnote-all-labels)
              (org-footnote--collect-references 'anonymous)
              (org-footnote--collect-definitions)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_missing_duplicate_normalize_sort_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"4\" \"3\" \"2\" \"1\") ((\"1\" #<marker in no buffer> t nil) (\"2\" #<marker in no buffer> t nil) (\"1\" #<marker in no buffer> t nil) (\"3\" #<marker in no buffer> t nil)) ((\"4\" . \"[fn:4] Unused def\") (\"3\" . \"[fn:3] Local def\") (\"2\" . \"[fn:2] DEFINITION NOT FOUND.\") (\"1\" . \"[fn:1] First Z\")) \"* H\\nFirst[fn:1] missing[fn:2] again[fn:1].\\n** Local\\nNested[fn:3]\\n\\n* Footnotes\\n\\n[fn:1] First Z\\n\\n[fn:2] DEFINITION NOT FOUND.\\n\\n[fn:3] Local def\\n\\n[fn:4] Unused def\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* H\n")
    (insert "First[fn:z] missing[fn:missing] again[fn:z].\n")
    (insert "** Local\n")
    (insert "Nested[fn:local]\n")
    (insert "[fn:local] Local def\n")
    (insert "* Footnotes\n")
    (insert "[fn:z] First Z\n")
    (insert "[fn:z] Duplicate Z\n")
    (insert "[fn:unused] Unused def\n")
    (let ((org-footnote-section "Footnotes")
          (org-footnote-fill-after-inline-note-extraction nil))
      (org-footnote-normalize)
      (org-footnote-sort)
      (list (org-footnote-all-labels)
            (org-footnote--collect-references 'anonymous)
            (org-footnote--collect-definitions)
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_action_context_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-auto-label)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Body\n")
    (insert "Paragraph anchor and old ref[fn:old].\n")
    (insert "Link [[https://example.org][link anchor]] text.\n")
    (insert "| table anchor | value |\n")
    (insert "#+begin_src emacs-lisp\n")
    (insert "src anchor\n")
    (insert "#+end_src\n")
    (insert "#+begin_verse\n")
    (insert "verse anchor\n")
    (insert "#+end_verse\n")
    (insert "* Footnotes\n")
    (insert "[fn:old] Old definition\n")
    (let ((probe
           (lambda (needle)
             (save-excursion
               (goto-char (point-min))
               (search-forward needle)
               (goto-char (match-beginning 0))
               (let ((context (org-element-context)))
                 (list needle
                       (org-element-type context)
                       (org-footnote-in-valid-context-p)
                       (org-footnote--allow-reference-p)
                       (org-footnote-at-reference-p)
                       (org-footnote-at-definition-p)))))))
          (org-footnote-auto-label 'confirm)
          (org-footnote-define-inline nil)
          (org-footnote-auto-adjust t)
          (org-footnote-section "Footnotes")
          (org-footnote-fill-after-inline-note-extraction nil))
      (let ((before (mapcar probe
                            '("Paragraph" "old ref" "link anchor"
                              "table anchor" "src anchor" "verse anchor"
                              "Old definition"))))
        (goto-char (point-min))
        (search-forward "Paragraph")
        (cl-letf (((symbol-function 'read-string)
                   (lambda (&rest _) "custom-label")))
          (org-footnote-new))
        (insert "Custom definition")
        (let ((after-new (buffer-substring-no-properties
                          (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "[fn:custom-label]")
          (org-footnote-action)
          (let ((after-action-def
                 (list (line-number-at-pos)
                       (org-footnote-at-definition-p))))
            (cl-letf (((symbol-function 'read-char-exclusive)
                       (lambda (&rest _) ?S)))
              (org-footnote-action t))
            (list before
                  after-new
                  after-action-def
                  (org-footnote-all-labels)
                  (org-footnote--collect-references 'anonymous)
                  (org-footnote--collect-definitions)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_local_normalize_nested_missing_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Chapter A\n")
    (insert "Top A[fn:a] inline[fn::Anon A with nested [fn:nested]].\n")
    (insert "** Child A\n")
    (insert "Child ref[fn:missing] and repeat[fn:a].\n")
    (insert "[fn:a] Local A definition\n")
    (insert "[fn:nested] Nested definition\n")
    (insert "* Chapter B\n")
    (insert "Top B[fn:b] and anonymous[fn::Anon B].\n")
    (insert "[fn:unused] Unused B definition\n")
    (insert "[fn:b] Bee definition\n")
    (let ((org-footnote-section nil)
          (org-footnote-fill-after-inline-note-extraction nil))
      (let ((before (list (org-footnote-all-labels)
                          (org-footnote--collect-references 'anonymous)
                          (org-footnote--collect-definitions))))
        (org-footnote-normalize)
        (let ((after-normalize
               (buffer-substring-no-properties (point-min) (point-max)))
              (labels-after-normalize (org-footnote-all-labels))
              (defs-after-normalize (org-footnote--collect-definitions)))
          (org-footnote-sort)
          (goto-char (point-min))
          (search-forward "Chapter B")
          (let ((next-b (org-footnote-get-next-reference nil nil
                                                         (save-excursion
                                                           (outline-next-heading)
                                                           (point))))
                (prev-global (org-footnote-get-next-reference nil t)))
            (list before
                  labels-after-normalize
                  defs-after-normalize
                  after-normalize
                  (org-footnote-all-labels)
                  (org-footnote--collect-references 'anonymous)
                  (org-footnote--collect-definitions)
                  (and next-b
                       (list (car next-b)
                             (line-number-at-pos (nth 1 next-b))))
                  (and prev-global
                       (list (car prev-global)
                             (line-number-at-pos (nth 1 prev-global))))
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_label_definition_section_adjust_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Body\n")
    (insert "A[fn:1] B[fn:3] inline[fn::Inline note].\n")
    (insert "* Footnotes\n")
    (insert "[fn:1] One definition\n")
    (insert "[fn:2] Unused two\n")
    (insert "[fn:3] Three definition\n")
    (insert "* Footnotes\n")
    (insert "[fn:dup] Duplicate section definition\n")
    (let ((org-footnote-section "Footnotes")
          (org-footnote-auto-adjust 'renumber)
          (org-footnote-fill-after-inline-note-extraction nil))
      (let ((initial
             (list (mapcar #'org-footnote-normalize-label
                           '("fn:abc" "  fn:spaced  " " plain " "   "))
                   (org-footnote-unique-label)
                   (org-footnote-unique-label '("1" "2" "4"))
                   (org-footnote-get-definition "fn:1")
                   (org-footnote-get-definition "3")
                   (org-footnote-get-definition "missing")
                   (org-footnote-all-labels)
                   (org-footnote--collect-definitions))))
        (let ((deleted-defs (org-footnote--collect-definitions t))
              (after-delete-defs
               (buffer-substring-no-properties (point-min) (point-max))))
          (org-footnote--clear-footnote-section)
          (let ((after-clear
                 (buffer-substring-no-properties (point-min) (point-max))))
            (goto-char (point-min))
            (search-forward "B")
            (org-footnote-new)
            (insert "New auto-adjust definition")
            (org-footnote-auto-adjust-maybe)
            (let ((after-adjust
                   (buffer-substring-no-properties (point-min) (point-max))))
              (list initial
                    deleted-defs
                    after-delete-defs
                    after-clear
                    (org-footnote-all-labels)
                    (org-footnote--collect-references 'anonymous)
                    (org-footnote--collect-definitions)
                    after-adjust)))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_export_numbering_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (require 'ox)
  (require 'ox-html)
  (require 'ox-ascii)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Foot Export\n")
    (insert "* First\n")
    (insert "Alpha[fn:a] inline[fn::Inline *bold* note] repeat[fn:a].\n")
    (insert "** Nested\n")
    (insert "Nested ref[fn:n] and anonymous[fn::Nested anon].\n")
    (insert "* Second\n")
    (insert "Beta[fn:b] late[fn:late].\n")
    (insert "* Footnotes\n")
    (insert "[fn:b] Bee definition\n")
    (insert "[fn:a] Aye definition\n")
    (insert "[fn:n] Nested definition\n")
    (insert "[fn:late] Late definition\n")
    (let ((org-footnote-section "Footnotes")
          (org-footnote-fill-after-inline-note-extraction nil)
          (org-export-with-toc nil))
      (let* ((tree (org-element-parse-buffer))
             (info (org-export-get-environment 'html nil nil))
             (refs
              (org-element-map tree 'footnote-reference
                (lambda (fn)
                  (list (org-element-property :label fn)
                        (org-element-property :type fn)
                        (org-export-get-footnote-number fn info tree)
                        (org-export-footnote-first-reference-p fn info tree)))))
             (defs-body
              (org-export-collect-footnote-definitions info tree t))
             (defs-normal
              (org-export-collect-footnote-definitions info tree nil))
             (def-a
              (org-export-get-footnote-definition
               (car (org-element-map tree 'footnote-reference
                      (lambda (fn)
                        (and (equal (org-element-property :label fn) "a")
                             fn))
                      nil t))
               info))
             (html-before
              (org-export-as 'html nil nil t '(:with-toc nil)))
             (ascii-before
              (org-export-as 'ascii nil nil t '(:with-toc nil))))
        (org-footnote-normalize)
        (goto-char (point-min))
        (search-forward "[fn:late]")
        (let ((late-delete (org-footnote-delete "late"))
              after-delete)
          (setq after-delete
                (buffer-substring-no-properties (point-min) (point-max)))
          (goto-char (point-min))
          (search-forward "Second")
          (search-forward "Beta")
          (let ((org-footnote-auto-label 'plain)
                (org-footnote-define-inline nil)
                (org-footnote-auto-adjust 'sort))
            (org-footnote-new)
            (insert "Replacement beta definition")
            (org-footnote-auto-adjust-maybe))
          (org-footnote-renumber-fn:N)
          (org-footnote-sort)
          (let* ((tree-after (org-element-parse-buffer))
                 (info-after
                  (org-export-get-environment 'ascii nil nil))
                 (refs-after
                  (org-element-map tree-after 'footnote-reference
                    (lambda (fn)
                      (list (org-element-property :label fn)
                            (org-element-property :type fn)
                            (org-export-get-footnote-number
                             fn info-after tree-after)
                            (org-export-footnote-first-reference-p
                             fn info-after tree-after)))))
                 (ascii-after
                  (org-export-as 'ascii nil nil t '(:with-toc nil))))
            (list refs
                  defs-body
                  defs-normal
                  (and def-a
                       (mapcar (lambda (el)
                                 (org-element-type el))
                               def-a))
                  (mapcar (lambda (needle)
                            (not (null
                                  (string-match-p needle html-before))))
                          '("footnotes" "Aye definition"
                            "Inline <b>bold</b> note"))
                  (mapcar (lambda (needle)
                            (not (null
                                  (string-match-p needle ascii-before))))
                          '("Aye definition" "Inline *bold* note"
                            "Nested definition"))
                  late-delete
                  after-delete
                  (org-footnote-all-labels)
                  (org-footnote--collect-references 'anonymous)
                  (org-footnote--collect-definitions)
                  refs-after
                   ascii-after
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_insert_sort_normalize_delete_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"[fn:alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Section\n")
    (insert "First ref[fn:beta] and second[fn:alpha].\n")
    (insert "Third inline[fn:inline:Inline note text].\n\n")
    (insert "[fn:alpha] Alpha definition.\n")
    (insert "[fn:beta] Beta definition with *bold*.\n")
    (let ((snap (lambda ()
                  (list (org-footnote-all-labels)
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))
      ;; Initial state
      (let ((initial (funcall snap)))
        ;; Insert a new footnote
        (goto-char (point-min))
        (search-forward "Third inline")
        (end-of-line)
        (org-footnote-new)
        (insert "New footnote body.")
        (let ((after-insert (funcall snap)))
          ;; Normalize
          (org-footnote-normalize)
          (let ((after-normalize (funcall snap)))
            ;; Sort
            (org-footnote-sort)
            (let ((after-sort (funcall snap)))
              ;; Delete beta footnote
              (org-footnote-delete "beta")
              (let ((after-delete (funcall snap)))
                ;; Action at footnote
                (goto-char (point-min))
                (search-forward "[fn:alpha")
                (beginning-of-line)
                (let ((action-result
                       (condition-case nil
                           (progn (org-footnote-action) 'ok)
                         (error 'error))))
                  (list initial
                        after-insert
                        after-normalize
                        after-sort
                        after-delete
                        action-result))))))))))"##,
        expect,
    );
}

#[test]
fn org_footnote_insert_normalize_sort_delete_multi_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"[fn:one\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "Text with refs[fn:one] and [fn:two] and [fn:three].\n\n")
    (insert "[fn:one] First footnote.\n")
    (insert "[fn:two] Second footnote.\n")
    (insert "[fn:three] Third footnote.\n")
    (let ((snap (lambda ()
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (let ((initial (funcall snap)))
        ;; Insert new footnote
        (goto-char (point-min))
        (search-forward "[fn:three]")
        (end-of-line)
        (org-footnote-new)
        (insert "Fourth footnote body.")
        (let ((after-insert (funcall snap)))
          ;; Normalize
          (org-footnote-normalize)
          (let ((after-normalize (funcall snap)))
            ;; Sort
            (org-footnote-sort)
            (let ((after-sort (funcall snap)))
              ;; Delete two
              (org-footnote-delete "two")
              (let ((after-delete (funcall snap)))
                ;; Goto footnote action
                (goto-char (point-min))
                (search-forward "[fn:one")
                (beginning-of-line)
                (let ((action-result
                       (condition-case nil
                           (progn (org-footnote-action) 'ok)
                         (error 'error))))
                  (list initial
                        after-insert
                        after-normalize
                        after-sort
                        after-delete
                        action-result
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))))"##,
        expect,
    );
}
