use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_cite_processor_declaration_and_plist_parse_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((:style \"author-year\" :notes \"t\" :foo \"bar\") (basic \"author-year\" nil) (biblatex \"bibstyle=authoryear\" \"citestyle=authoryear\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'oc)
  (list (org-cite--parse-as-plist ":style author-year :notes t :foo bar")
        (org-cite-read-processor-declaration "basic author-year")
        (org-cite-read-processor-declaration
         "biblatex bibstyle=authoryear citestyle=authoryear")))"#,
        expect,
    );
}

#[test]
fn org_cite_bibliography_and_reference_boundaries_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"refs.bib more.json\") ((\"t\" (\"doe2020\" \"roe2021\") (40 . 76) (nil)) (nil (\"solo\") (81 . 93) (nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (with-temp-buffer
    (org-mode)
    (insert "#+bibliography: refs.bib more.json\n")
    (insert "Text [cite/t:@doe2020 p. 4; see @roe2021] and [cite:@solo].\n")
    (let* ((tree (org-element-parse-buffer))
           (citations
            (org-element-map tree 'citation
              (lambda (citation)
                (list (org-element-property :style citation)
                      (org-cite-get-references citation t)
                      (let ((bounds (org-cite-boundaries citation)))
                        (cons (- (car bounds) (point-min))
                              (- (cdr bounds) (point-min))))
                      (org-cite-main-affixes citation))))))
      (list (org-cite-list-bibliography-files)
            citations))))"##,
        expect,
    );
}

#[test]
fn org_cite_basic_bibtex_json_parse_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 58 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (require 'oc-basic)
  (require 'ox-org)
  (let* ((root (make-temp-file "org-cite-basic" t))
         (bib (expand-file-name "refs.bib" root))
         (json (expand-file-name "refs.json" root))
         (org-cite-global-bibliography nil)
         (org-cite-export-processors '((t basic))))
    (unwind-protect
        (progn
          (with-temp-file bib
            (insert "@string{j = {Journal One}}\n")
            (insert "@article{smith2020,\n")
            (insert "  author = {Smith, Ada and Roe, Bob},\n")
            (insert "  title = {Alpha Study},\n")
            (insert "  journal = j,\n")
            (insert "  year = {2020}}\n")
            (insert "@book{doe2019,\n")
            (insert "  editor = {Doe, Dana},\n")
            (insert "  title = {Beta Book},\n")
            (insert "  publisher = {Press},\n")
            (insert "  year = {2019}}\n"))
          (with-temp-file json
            (insert "[{\"id\":\"json2021\",")
            (insert "\"author\":[{\"family\":\"Young\",\"given\":\"Yara\"}],")
            (insert "\"title\":\"Gamma JSON\",")
            (insert "\"issued\":{\"date-parts\":[[2021]]},")
            (insert "\"publisher\":\"JSON Press\"}]"))
          (with-temp-buffer
            (org-mode)
            (insert "#+cite_export: basic author-year numeric\n")
            (insert "#+bibliography: " bib " " json "\n")
            (insert "Lead [cite:@smith2020; @json2021 p. 7] and ")
            (insert "[cite/author:@doe2019].\n")
            (insert "#+print_bibliography:\n")
            (let* ((info (org-export-get-environment 'org nil))
                   (keys (org-cite-list-keys info))
                   (numbers (mapcar (lambda (key)
                                      (list key (org-cite-basic--key-number
                                                 key info)))
                                    keys))
                   (parsed (mapcar
                            (lambda (key)
                              (list key
                                    (org-cite-basic--get-author key info 'raw)
                                    (org-cite-basic--get-year key info 'no-suffix)
                                    (org-cite-basic--get-field
                                     'title key info 'raw)))
                            (sort (copy-sequence keys) #'string<)))
                   (out (org-export-as 'org nil nil t nil)))
              (list (org-cite-list-bibliography-files)
                    keys
                    numbers
                    parsed
                    out)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_cite_basic_note_numeric_bibliography_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 52 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (require 'oc-basic)
  (require 'ox-ascii)
  (let* ((root (make-temp-file "org-cite-export" t))
         (bib (expand-file-name "refs.bib" root))
         (org-cite-global-bibliography nil)
         (org-cite-export-processors '((ascii basic))))
    (unwind-protect
        (progn
          (with-temp-file bib
            (insert "@article{alpha,\n")
            (insert "  author = {Alpha, Ann},\n")
            (insert "  title = {First Paper},\n")
            (insert "  journal = {J},\n")
            (insert "  year = {2020}}\n")
            (insert "@article{beta,\n")
            (insert "  author = {Beta, Ben},\n")
            (insert "  title = {Second Paper},\n")
            (insert "  journal = {J},\n")
            (insert "  year = {2021}}\n")
            (insert "@article{gamma,\n")
            (insert "  author = {Gamma, Gail},\n")
            (insert "  title = {Third Paper},\n")
            (insert "  journal = {J},\n")
            (insert "  year = {2022}}\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+cite_export: basic numeric text/bare\n")
            (insert "#+bibliography: " bib "\n")
            (insert "* Cites\n")
            (insert "Text [cite:@beta; @alpha] sentence.\n")
            (insert "Note ending.[cite/note:@gamma] next.\n")
            (insert "[cite/n:@alpha] keeps key only for bibliography.\n")
            (insert "#+print_bibliography: :style numeric\n")
            (let* ((org-export-with-toc nil)
                   (org-ascii-text-width 80)
                   (org-ascii-charset 'utf-8)
                   (info (org-export-get-environment 'ascii nil))
                   (citations
                    (mapcar
                     (lambda (cite)
                       (list (org-element-property :style cite)
                             (org-cite-get-references cite t)
                             (org-cite-inside-footnote-p cite)
                             (org-cite-citation-style cite info)))
                     (org-cite-list-citations info)))
                   (keys (org-cite-list-keys info))
                   (output (org-export-as 'ascii nil nil t nil)))
              (list keys citations output)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_cite_delete_reference_and_citation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-cite-delete-reference)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (with-temp-buffer
    (org-mode)
    (insert "Before [cite:see @one p. 1; compare @two; @three] after.\n")
    (insert "Solo [cite:@solo] done.\n")
    (goto-char (point-min))
    (search-forward "@two")
    (let* ((ref (org-element-context))
           (cite (org-element-lineage ref '(citation))))
      (org-cite-delete-reference ref)
      (goto-char (point-min))
      (search-forward "@solo")
      (org-cite-delete-citation (org-element-lineage
                                 (org-element-context)
                                 '(citation)))
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (remaining
              (org-element-map tree 'citation
                (lambda (citation)
                  (list (org-element-property :style citation)
                        (org-cite-get-references citation t)
                        (org-cite-main-affixes citation)))))
             (objects
              (org-element-map tree '(citation citation-reference)
                (lambda (obj)
                  (list (org-element-type obj)
                        (org-element-property :key obj)
                        (org-element-property :prefix obj)
                        (org-element-property :suffix obj))))))
        (list remaining
              objects
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_cite_custom_processor_note_adjust_wrap_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((original-processors org-cite--processors)
        (calls nil))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (org-cite-register-processor
           'probe
           :cite-styles '((nil "plain" "Plain")
                          ("n" "note" "Note")
                          ("author" "author" "Author"))
           :export-citation
           (lambda (citation style _ backend info)
             (push (list 'export
                         style
                         backend
                         (org-cite-get-references citation t)
                         (org-cite-main-affixes citation))
                   calls)
             (format "<%s:%s>"
                     (or style "plain")
                     (mapconcat #'identity
                                (org-cite-get-references citation t)
                                ",")))
           :export-bibliography
           (lambda (keys files style props backend info)
             (push (list 'bibliography keys files style props backend
                         (plist-get info :cite-export))
                   calls)
             (format "BIB:%s:%s"
                     style
                     (mapconcat #'identity keys ",")))
           :activate (lambda (citation)
                       (push (list 'activate
                                   (org-cite-get-references citation t))
                             calls))
           :follow (lambda (datum arg)
                     (push (list 'follow
                                 (org-cite-get-references datum t)
                                 arg)
                           calls))
           :insert (lambda (&optional arg)
                     (push (list 'insert arg) calls)))
          (insert "#+cite_export: probe n :alpha beta\n")
          (insert "Sentence [cite:@alpha] then note.[cite/n:@beta p. 4] next\n")
          (insert "Tail [cite/author:see @gamma; compare @delta].\n")
          (let* ((org-cite-export-processors '((org probe)))
                 (org-cite-note-rules '((t . ((t . before)))))
                 (info (org-export-get-environment 'org nil))
                 (citations (org-cite-list-citations info))
                 (before
                  (mapcar (lambda (citation)
                            (list (org-element-property :style citation)
                                  (org-cite-get-references citation t)
                                  (org-cite-main-affixes citation)
                                  (org-cite-citation-style citation info)
                                  (org-cite-inside-footnote-p citation)
                                  (let ((bounds
                                         (org-cite-boundaries citation)))
                                    (cons (- (car bounds) (point-min))
                                          (- (cdr bounds) (point-min))))))
                          citations)))
            (org-cite-adjust-note (nth 1 citations) info)
            (setq citations
                  (org-cite-list-citations
                   (org-export-get-environment 'org nil)))
            (org-cite-wrap-citation (nth 1 citations) info)
            (let* ((info-after (org-export-get-environment 'org nil))
                   (after
                    (mapcar (lambda (citation)
                              (list (org-element-property :style citation)
                                    (org-cite-get-references citation t)
                                    (org-cite-inside-footnote-p citation t)))
                            (org-cite-list-citations info-after)))
                   (exported-citations
                    (mapcar (lambda (citation)
                              (funcall
                               (org-cite-processor-export-citation
                                (org-cite-get-processor 'probe))
                               citation
                               (org-cite-citation-style citation info-after)
                               nil 'org info-after))
                            (org-cite-list-citations info-after)))
                   (bibliography
                    (funcall
                     (org-cite-processor-export-bibliography
                      (org-cite-get-processor 'probe))
                     (org-cite-list-keys info-after)
                     (org-cite-list-bibliography-files)
                     (org-cite-bibliography-style info-after)
                     (org-cite-bibliography-properties
                      (car (plist-get info-after :print-bibliography)))
                     'org info-after)))
              (list before
                    after
                    exported-citations
                    bibliography
                    (org-cite-supported-styles '(probe))
                    (mapcar (lambda (cap)
                              (org-cite-processor-has-capability-p
                               'probe cap))
                            '(activate export follow insert))
                    (nreverse calls)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
      (setq org-cite--processors original-processors))))"##,
        expect,
    );
}

#[test]
fn org_cite_insert_processor_affix_boundaries_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 104 56)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((original-processors org-cite--processors)
        (keys '("new-before" "replace-key" "new-after"
                "global-prefix" "global-suffix"
                "fresh-a" "fresh-b"))
        (styles '("author" "text/bare"))
        (calls nil))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (org-cite-register-processor
           'insert-probe
           :cite-styles '((nil "plain" "Plain")
                          ("author" "author" "Author")
                          ("text/bare" "text bare" "Text bare"))
           :insert
           (org-cite-make-insert-processor
            (lambda (multiple)
              (push (list 'select-key multiple) calls)
              (if multiple
                  (list (pop keys) (pop keys))
                (pop keys)))
            (lambda (citation)
              (push (list 'select-style
                          (and citation
                               (org-cite-get-references citation t)))
                    calls)
              (pop styles))))
          (let ((org-cite-insert-processor 'insert-probe))
            (insert "Lead [cite:see @one p. 1; compare @two p. 2] tail.\n")
            (insert "Solo [cite:@solo].\n")
            (insert "Plain paragraph end")
            ;; Before @one: insert a new reference before the first one.
            (goto-char (point-min))
            (search-forward "@one")
            (backward-char 4)
            (org-cite-insert nil)
            ;; Inside @one: replace key while preserving affixes.
            (goto-char (point-min))
            (search-forward "@one")
            (backward-char 2)
            (org-cite-insert nil)
            ;; After suffix of @two: insert a reference after it.
            (goto-char (point-min))
            (search-forward "p. 2")
            (org-cite-insert nil)
            ;; On global prefix: insert before first reference.
            (goto-char (point-min))
            (search-forward "see ")
            (org-cite-insert nil)
            ;; On global suffix: insert after last reference.
            (goto-char (point-min))
            (search-forward " tail")
            (backward-char 1)
            (org-cite-insert nil)
            ;; Elsewhere with ARG: insert a fresh styled citation.
            (goto-char (point-max))
            (insert " ")
            (org-cite-insert '(4))
            ;; Delete the solo citation through insert processor ARG.
            (goto-char (point-min))
            (search-forward "@solo")
            (org-cite-insert '(4))
            (font-lock-ensure (point-min) (point-max))
            (let* ((tree (org-element-parse-buffer))
                   (citations
                    (org-element-map tree 'citation
                      (lambda (citation)
                        (list (org-element-property :style citation)
                              (org-cite-get-references citation t)
                              (org-cite-main-affixes citation)
                              (let ((bounds (org-cite-boundaries citation)))
                                (cons (- (car bounds) (point-min))
                                      (- (cdr bounds) (point-min))))
                              (mapcar
                               (lambda (ref)
                                 (list (org-element-property :key ref)
                                       (org-element-property :prefix ref)
                                       (org-element-property :suffix ref)
                                       (let ((bounds
                                              (org-cite-key-boundaries ref)))
                                         (cons (- (car bounds) (point-min))
                                               (- (cdr bounds) (point-min))))))
                               (org-cite-get-references citation))))))
                   (faces
                    (mapcar
                     (lambda (needle)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward needle)
                         (list needle
                               (get-text-property (match-beginning 0) 'face)
                               (get-text-property (match-beginning 0)
                                                  'font-lock-fontified))))
                     '("@new-before" "@replace-key" "@new-after"
                       "@global-prefix" "@global-suffix" "@fresh-a"))))
              (list citations
                    faces
                    (nreverse calls)
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (setq org-cite--processors original-processors))))"##,
        expect,
    );
}

#[test]
fn org_cite_basic_activation_follow_completion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"<root>/refs.bib\") (\"alpha2020\" \"beta2021\") nil (\"alpha2020\") ((\"alpha2020\" \"Alpha, Ann\" #(\"2020\" 0 4 (:parent nil)) #(\"Known Alpha\" 0 11 (:parent nil))) (\"beta2021\" \"Beta, Bob\" #(\"2021\" 0 4 (:parent nil)) #(\"Known Beta\" 0 10 (:parent nil)))) (((\"alpha2020\" \"beta2021\") (nil) (0 . 37)) ((\"alpah2020\" \"missing\") (nil) (0 . 27))) ((\"alpha2020\" (org-cite-key org-cite) highlight #(\"Alpha. Known Alpha, J, 2020.\" 0 28 (:parent nil)) t) (\"alpah2020\" (error org-cite) highlight \"Suggestions (mouse-1 to substitute): alpha2020\" t) (\"missing\" (error org-cite) highlight nil t)) (\"refs.bib\" \"@article{alpha2020,\") (user-error \"Cannot find citation key: \\\"alpah2020\\\"\") \"#+bibliography: <root>/refs.bib\\nKnown [cite:see @alpha2020 p. 4; @beta2021] Missing [cite:@alpah2020; @missing].\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (require 'oc-basic)
  (let* ((root (make-temp-file "org-cite-activate" t))
         (bib (expand-file-name "refs.bib" root))
         (org-cite-global-bibliography nil)
         (org-cite-activate-processor 'basic)
         (org-cite-follow-processor 'basic))
    (unwind-protect
        (progn
          (with-temp-file bib
            (insert "@article{alpha2020,\n")
            (insert "  author = {Alpha, Ann},\n")
            (insert "  title = {Known Alpha},\n")
            (insert "  journal = {J},\n")
            (insert "  year = {2020}}\n\n")
            (insert "@book{beta2021,\n")
            (insert "  author = {Beta, Bob},\n")
            (insert "  title = {Known Beta},\n")
            (insert "  publisher = {Press},\n")
            (insert "  year = {2021}}\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+bibliography: " bib "\n")
            (insert "Known [cite:see @alpha2020 p. 4; @beta2021] ")
            (insert "Missing [cite:@alpah2020; @missing].\n")
            (font-lock-ensure (point-min) (point-max))
            (let* ((tree (org-element-parse-buffer))
                   (citations (org-element-map tree 'citation #'identity))
                   (refs (org-element-map tree 'citation-reference
                           #'identity))
                   (info (org-export-get-environment 'org nil))
                   (completion-table
                    (org-cite-basic--key-completion-table))
                   (props
                    (mapcar
                     (lambda (key)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward (concat "@" key))
                         (let ((pos (1- (point))))
                           (list key
                                 (get-text-property pos 'face)
                                 (get-text-property pos 'mouse-face)
                                 (let ((help
                                        (get-text-property pos 'help-echo)))
                                   (if (stringp help)
                                       (neovm--oracle-coalesce-string-properties
                                        help)
                                     help))
                                 (keymapp
                                  (get-text-property pos 'keymap))))))
                     '("alpha2020" "alpah2020" "missing"))))
              (let ((follow-alpha
                     (save-excursion
                       (org-cite-basic-goto (car refs) nil)
                       (list (file-name-nondirectory
                              (or (buffer-file-name) ""))
                             (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position)))))
                    (follow-missing
                     (condition-case err
                         (save-excursion
                           (org-cite-basic-goto (nth 2 refs) nil)
                           'ok)
                       (error (cons (car err) (cdr err))))))
                (list (mapcar (lambda (path)
                                (replace-regexp-in-string
                                 (regexp-quote root) "<root>" path))
                              (org-cite-list-bibliography-files))
                      (org-cite-basic--all-keys)
                      (sort (all-completions "a" completion-table) #'string<)
                      (org-cite-basic--close-keys
                       "alpah2020" (org-cite-basic--all-keys))
                      (mapcar (lambda (key)
                                (list key
                                      (org-cite-basic--get-author
                                       key info 'raw)
                                      (org-cite-basic--get-year
                                       key info 'no-suffix)
                                      (org-cite-basic--get-field
                                       'title key info 'raw)))
                              '("alpha2020" "beta2021"))
                      (mapcar (lambda (citation)
                                (let* ((begin
                                        (org-element-property :begin citation))
                                       (bounds
                                        (org-cite-boundaries citation)))
                                  (list
                                   (org-cite-get-references citation t)
                                   (org-cite-main-affixes citation)
                                   (and bounds
                                        (cons (- (car bounds) begin)
                                              (- (cdr bounds) begin))))))
                              citations)
                      props
                      follow-alpha
                      follow-missing
                      (replace-regexp-in-string
                       (regexp-quote root)
                       "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))))
      (dolist (buf (list (get-file-buffer bib)))
        (when buf (kill-buffer buf)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_cite_basic_disambiguation_multibackend_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 100 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (require 'oc-basic)
  (require 'ox-html)
  (require 'ox-ascii)
  (require 'ox-latex)
  (let* ((root (make-temp-file "org-cite-disambig" t))
         (bib (expand-file-name "refs.bib" root))
         (org-cite-global-bibliography nil)
         (org-cite-export-processors
          '((html basic) (ascii basic) (latex basic))))
    (unwind-protect
        (progn
          (with-temp-file bib
            (insert "@article{smith2020a,\n")
            (insert "  author = {Smith, Ada and Roe, Bob},\n")
            (insert "  title = {Alpha Study},\n")
            (insert "  journal = {Journal},\n")
            (insert "  year = {2020}}\n")
            (insert "@article{smith2020b,\n")
            (insert "  author = {Smith, Ada and Roe, Bob},\n")
            (insert "  title = {Beta Study},\n")
            (insert "  journal = {Journal},\n")
            (insert "  year = {2020}}\n")
            (insert "@book{doe2019,\n")
            (insert "  editor = {Doe, Dana},\n")
            (insert "  title = {Edited Volume},\n")
            (insert "  publisher = {Press},\n")
            (insert "  year = {2019}}\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: Citation Matrix\n")
            (insert "#+OPTIONS: toc:nil num:nil\n")
            (insert "#+cite_export: basic author-year\n")
            (insert "#+bibliography: " bib "\n")
            (insert "* Main\n")
            (insert "Author text [cite/author:@smith2020a; @smith2020b] and ")
            (insert "narrative [cite/t:see @smith2020a p. 4; also @doe2019].\n")
            (insert "Bare [cite:@smith2020b] plus note.[cite/note:@doe2019 chap. 2]\n")
            (insert "#+print_bibliography: :style author-year\n")
            (let* ((html-info (org-export-get-environment 'html nil))
                   (citations
                    (mapcar
                     (lambda (citation)
                       (list (org-element-property :style citation)
                             (org-cite-get-references citation t)
                             (org-cite-main-affixes citation)
                             (org-cite-citation-style citation html-info)
                             (org-cite-inside-footnote-p citation)
                             (org-cite-boundaries citation)))
                     (org-cite-list-citations html-info)))
                   (keys (org-cite-list-keys html-info))
                   (fields
                    (mapcar
                     (lambda (key)
                       (list key
                             (org-cite-basic--key-number key html-info)
                             (org-cite-basic--get-author key html-info 'raw)
                             (org-cite-basic--get-year key html-info nil)
                             (org-cite-basic--get-year key html-info 'no-suffix)
                             (org-cite-basic--get-field
                              'title key html-info 'raw)
                             (org-cite-basic--print-entry
                              (org-cite-basic--get-entry key html-info)
                              'author-year
                              html-info)))
                     (sort (copy-sequence keys) #'string<)))
                   (html (replace-regexp-in-string
                          "org[[:alnum:]]+"
                          "org-id"
                          (org-export-as 'html nil nil t
                                         '(:with-toc nil))))
                   (ascii (let ((org-ascii-charset 'utf-8))
                            (org-export-as 'ascii nil nil t
                                           '(:with-toc nil))))
                   (latex (replace-regexp-in-string
                           "sec:org[[:alnum:]]+"
                           "sec:org-id"
                           (org-export-as 'latex nil nil t
                                          '(:with-toc nil)))))
              (list citations
                    keys
                    fields
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle html))))
                            '("Citation Matrix" "Smith" "2020a" "2020b"
                              "Edited Volume" "Alpha Study" "Beta Study"))
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle ascii))))
                            '("Smith" "2020a" "2020b" "Edited Volume"
                              "Alpha Study" "Beta Study"))
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle latex))))
                            '("Smith" "2020a" "2020b" "Edited Volume"
                              "Alpha Study" "Beta Study"))
                    html
                    ascii
                    latex)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_citation_parse_reference_style_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((nil nil (\"doe2020\")) (\"t\" nil (\"roe2021\" \"smith2022\")) (nil nil (\"solo\"))) ((\"doe2020\") (\"roe2021\") (\"smith2022\") (\"solo\")) (org-data section keyword keyword headline plain-text section paragraph plain-text citation citation-reference plain-text citation citation-reference citation-reference plain-text plain-text plain-text citation citation-reference plain-text) \"#+cite_export: basic author-year\\n#+bibliography: refs.bib\\n\\n* Section\\nText [cite:@doe2020] and [cite/t:@roe2021; see @smith2022 p. 10].\\nPlain [cite:@solo].\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (with-temp-buffer
    (org-mode)
    (insert "#+cite_export: basic author-year\n")
    (insert "#+bibliography: refs.bib\n\n")
    (insert "* Section\n")
    (insert "Text [cite:@doe2020] and [cite/t:@roe2021; see @smith2022 p. 10].\n")
    (insert "Plain [cite:@solo].\n\n")
    (let* ((tree (org-element-parse-buffer))
           (citations
            (org-element-map tree 'citation
              (lambda (c)
                (list (org-element-property :style c)
                      (org-element-property :prefix c)
                      (org-cite-get-references c t)))))
           (refs
            (org-element-map tree 'citation-reference
              (lambda (cr)
                (list (org-element-property :key cr)))))
           (all-types
            (mapcar #'org-element-type
                    (org-element-map tree t #'identity))))
      (list citations refs all-types
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}
