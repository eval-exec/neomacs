//! Recovered inline Org-mode oracle parity tests.
//!
//! These tests were authored directly in `org_mode/mod.rs` alongside the
//! submodule declarations. They are kept here as their own submodule so the
//! parent `mod.rs` can stay a pure module index.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_element_headline_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+begin_src emacs-lisp :results value replace\\n(+ 2 3)\\n#+end_src\\n\\n#+RESULTS:\\n: 5\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task :work:\n")
    (insert "SCHEDULED: <2026-05-26 Tue>\n")
    (insert "Body\n")
    (insert "** DONE Child\n")
    (let ((out nil))
      (org-element-map (org-element-parse-buffer) 'headline
        (lambda (headline)
          (push (list (org-element-property :level headline)
                      (org-element-property :todo-keyword headline)
                      (org-element-property :raw-value headline)
                      (org-element-property :tags headline))
                out)))
      (nreverse out))))"#,
    );
}

#[test]
fn org_todo_keyword_edit_preserves_plain_buffer_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-log-done nil)
          (org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE" "CANCELED"))))
      (org-mode)
      (insert "* TODO Task\n")
      (goto-char (point-min))
      (org-todo "DONE")
      (list (substring-no-properties (org-get-todo-state))
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
    );
}

#[test]
fn org_table_align_formats_columns_and_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Qty |\n")
    (insert "| apple | 2 |\n")
    (insert "| banana | 10 |\n")
    (goto-char (point-min))
    (org-table-align)
    (buffer-substring-no-properties (point-min) (point-max))))"#,
    );
}

#[test]
fn org_element_link_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "See [[https://example.org/path][Example]] and [[file:notes.org::*Target][Target]].\n")
    (let ((out nil))
      (org-element-map (org-element-parse-buffer) 'link
        (lambda (link)
          (push (list (org-element-property :type link)
                      (org-element-property :path link)
                      (org-element-property :raw-link link)
                      (org-element-property :search-option link)
                      (buffer-substring-no-properties
                       (org-element-property :contents-begin link)
                       (org-element-property :contents-end link)))
                out)))
      (nreverse out))))"#,
    );
}

#[test]
fn org_element_timestamp_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "Deadline <2026-05-26 Tue 12:34-13:45> and inactive [2026-06-01 Mon]\n")
    (let ((out nil))
      (org-element-map (org-element-parse-buffer) 'timestamp
        (lambda (timestamp)
          (push (list (org-element-property :type timestamp)
                      (org-element-property :year-start timestamp)
                      (org-element-property :month-start timestamp)
                      (org-element-property :day-start timestamp)
                      (org-element-property :hour-start timestamp)
                      (org-element-property :minute-start timestamp)
                      (org-element-property :hour-end timestamp)
                      (org-element-property :minute-end timestamp))
                out)))
      (nreverse out))))"#,
    );
}

#[test]
fn org_babel_emacs_lisp_result_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+begin_src emacs-lisp :results value replace\n")
    (insert "(+ 2 3)\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (let ((org-confirm-babel-evaluate nil))
      (org-babel-execute-src-block))
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_subtree_cut_paste_preserves_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 \"Alpha\") (2 \"A1\") (2 \"A2\") (1 \"Gamma\") (1 \"Beta\") (2 \"B1\")) \"* Alpha\\n** A1\\nbody A1\\n** A2\\n* Gamma\\n* Beta\\n** B1\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "** A1\nbody A1\n")
    (insert "** A2\n")
    (insert "* Beta\n")
    (insert "** B1\n")
    (insert "* Gamma\n")
    (goto-char (point-min))
    (search-forward "* Beta")
    (beginning-of-line)
    (org-cut-subtree)
    (goto-char (point-max))
    (org-paste-subtree 1)
    (let ((headlines nil))
      (org-element-map (org-element-parse-buffer) 'headline
        (lambda (headline)
          (push (list (org-element-property :level headline)
                      (org-element-property :raw-value headline))
                headlines)))
      (list (nreverse headlines)
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_properties_tags_and_todo_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Ada\" \"2:00\" (\"work\" \"urgent\") \"DONE\" \"* Project :root:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n** DONE Design                                                  :work:urgent:\\nSCHEDULED: <2026-05-26 Tue>\\n:PROPERTIES:\\n:Effort:   2:00\\n:END:\\n** WAIT Review\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-log-done nil)
          (org-use-property-inheritance t))
      (org-mode)
      (insert "* Project :root:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** TODO Design :work:\n")
      (insert "SCHEDULED: <2026-05-26 Tue>\n")
      (insert "** WAIT Review\n")
      (goto-char (point-min))
      (search-forward "Design")
      (beginning-of-line)
      (org-todo "DONE")
      (org-set-property "Effort" "2:00")
      (org-set-tags '("work" "urgent"))
      (list (org-entry-get nil "Owner" t)
            (org-entry-get nil "Effort")
            (org-get-tags nil t)
            (substring-no-properties (org-get-todo-state))
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_nested_checkbox_counts_after_toggles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Tasks [1/2]\\n- [X] first\\n  - [X] child\\n- [ ] second\\n\" (on on off))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Tasks [0/2]\n")
    (insert "- [ ] first\n")
    (insert "  - [ ] child\n")
    (insert "- [ ] second\n")
    (goto-char (point-min))
    (search-forward "first")
    (beginning-of-line)
    (org-toggle-checkbox)
    (search-forward "child")
    (beginning-of-line)
    (org-toggle-checkbox)
    (goto-char (point-min))
    (org-update-checkbox-count)
    (list (buffer-substring-no-properties (point-min) (point-max))
          (org-element-map (org-element-parse-buffer) 'item
            (lambda (item)
              (org-element-property :checkbox item))))))"#,
        expect,
    );
}

#[test]
fn org_table_multi_formula_recalculation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"| item  | value | tax |\\n|-------+-------+-----|\\n| a     |     2 |   1 |\\n| b     |     3 |   2 |\\n|-------+-------+-----|\\n| total |     5 |   3 |\\n#+TBLFM: @>$2=vsum(@2..@-1)::@>$3=vsum(@2..@-1)\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| item | value | tax |\n")
    (insert "|------+-------+-----|\n")
    (insert "| a | 2 | 1 |\n")
    (insert "| b | 3 | 2 |\n")
    (insert "|------+-------+-----|\n")
    (insert "| total |  |  |\n")
    (insert "#+TBLFM: @>$2=vsum(@2..@-1)::@>$3=vsum(@2..@-1)\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_document_element_mix_with_properties_blocks_and_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((keyword \"TITLE\" nil nil \"Demo\") (keyword \"AUTHOR\" nil nil \"Ada\") (headline nil \"Build\" nil nil) (planning nil nil nil nil) (property-drawer nil nil nil nil) (node-property \"ID\" nil nil \"build-1\") (paragraph nil nil nil nil) (paragraph nil nil nil nil) (src-block nil nil \"emacs-lisp\" \"(+ 1 2)\\n\") (table nil nil nil nil) (footnote-definition nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Demo\n")
    (insert "#+AUTHOR: Ada\n")
    (insert "* TODO Build :tag:\n")
    (insert "DEADLINE: <2026-05-27 Wed>\n")
    (insert ":PROPERTIES:\n:ID: build-1\n:END:\n")
    (insert "Paragraph with [fn:1].\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "| a | b |\n| 1 | 2 |\n")
    (insert "[fn:1] Footnote text\n")
    (let ((tree (org-element-parse-buffer))
          (out nil))
      (dolist (type '(keyword headline planning property-drawer node-property
                      paragraph src-block table footnote-definition))
        (org-element-map tree type
          (lambda (element)
            (push (list type
                        (org-element-property :key element)
                        (org-element-property :raw-value element)
                        (org-element-property :language element)
                        (org-element-property :value element))
                  out))))
      (nreverse out))))"##,
        expect,
    );
}

#[test]
fn org_html_export_markup_and_link_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t 264)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Demo\n")
    (insert "* Head\n")
    (insert "Paragraph with *bold* and [[https://example.org][link]].\n")
    (let* ((org-export-with-toc nil)
           (org-export-show-temporary-export-buffer nil)
           (html (org-export-as 'html nil nil t nil)))
      (list (not (null (string-match-p "<h2" html)))
            (not (null (string-match-p "Head</h2>" html)))
            (not (null (string-match-p "<b>bold</b>" html)))
            (not (null (string-match-p "<a href=\"https://example.org\">link</a>" html)))
            (length html)))))"##,
        expect,
    );
}

#[test]
fn org_agenda_file_schedule_deadline_and_tags_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t \"3 days-agenda (W22):\\nWednesday  27 May 2026\\nProbe:   9:00...... Scheduled:  TODO Write report                        :work:\\nThursday   28 May 2026\\nProbe:  Deadline:   WAIT Blocked                                         :home:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (let* ((file (make-temp-file "org-agenda-probe" nil ".org"
                               "#+CATEGORY: Probe
* TODO Write report :work:
SCHEDULED: <2026-05-27 Wed 09:00>
:PROPERTIES:
:Effort: 1:30
:END:
* WAIT Blocked :home:
DEADLINE: <2026-05-28 Thu>
* DONE Finished :work:
CLOSED: [2026-05-26 Tue]
"))
         (org-agenda-files (list file))
         (org-agenda-span 3)
         (org-agenda-start-day "2026-05-27")
         (org-agenda-start-on-weekday nil)
         (org-agenda-use-time-grid nil)
         (org-agenda-show-all-dates nil)
         (org-agenda-prefix-format "%-8:c%?-12t% s")
         (org-agenda-sorting-strategy '((agenda time-up priority-down category-keep))))
    (unwind-protect
        (progn
          (org-agenda-list nil "2026-05-27" 3)
          (with-current-buffer org-agenda-buffer-name
            (list (not (null (string-match-p "Write report" (buffer-string))))
                  (not (null (string-match-p "Blocked" (buffer-string))))
                  (not (null (string-match-p "Probe" (buffer-string))))
                  (buffer-substring-no-properties (point-min) (point-max)))))
      (when (get-buffer org-agenda-buffer-name)
        (kill-buffer org-agenda-buffer-name))
      (delete-file file))))"##,
        expect,
    );
}

#[test]
fn org_clock_table_data_from_logbook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (135 ((1 \"Project\" 135) (2 \"Task A\" 90) (2 \"Task B\" 45)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "* Project\n")
    (insert "** Task A\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\n")
    (insert ":END:\n")
    (insert "** Task B\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\n")
    (insert ":END:\n")
    (let* ((data (org-clock-get-table-data
                  nil (list :maxlevel 3 :scope 'buffer :block nil)))
           (total (nth 1 data))
           (rows (mapcar (lambda (row)
                           (list (nth 0 row)
                                 (substring-no-properties (nth 1 row))
                                 (nth 4 row)))
                         (nth 2 data))))
      (list total rows))))"#,
        expect,
    );
}

#[test]
fn org_babel_tangle_multiple_emacs_lisp_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"el\") \"(defun alpha () 1)\\n\\n(defun beta () (+ (alpha) 2))\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((src (make-temp-file "org-tangle-src" nil ".org"))
         (out (make-temp-file "org-tangle" nil ".el"))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (with-current-buffer (find-file-noselect src)
          (erase-buffer)
          (org-mode)
          (insert "#+PROPERTY: header-args:emacs-lisp :comments no\n")
          (insert "#+begin_src emacs-lisp :tangle " out "\n")
          (insert "(defun alpha () 1)\n")
          (insert "#+end_src\n")
          (insert "#+begin_src emacs-lisp :tangle " out "\n")
          (insert "(defun beta () (+ (alpha) 2))\n")
          (insert "#+end_src\n")
          (save-buffer)
          (let ((files (org-babel-tangle)))
            (list (mapcar #'file-name-extension files)
                  (with-temp-buffer
                    (insert-file-contents out)
                    (buffer-string)))))
      (when (get-file-buffer src) (kill-buffer (get-file-buffer src)))
      (when (file-exists-p src) (delete-file src))
      (when (file-exists-p out) (delete-file out)))))"##,
        expect,
    );
}

#[test]
fn org_footnote_normalize_and_sort_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"2\" \"1\") \"* Notes\\nFirst ref[fn:1] and second[fn:2].\\n\\n* Footnotes\\n\\n[fn:1] Alpha text\\n\\n[fn:2] Beta text\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-footnote)
  (with-temp-buffer
    (org-mode)
    (insert "* Notes\n")
    (insert "First ref[fn:alpha] and second[fn:beta].\n\n")
    (insert "[fn:beta] Beta text\n")
    (insert "[fn:alpha] Alpha text\n")
    (org-footnote-normalize)
    (org-footnote-sort)
    (list (org-footnote-all-labels)
          (buffer-substring-no-properties (point-min) (point-max)))))"#,
        expect,
    );
}

#[test]
fn org_archive_to_sibling_normalized_timestamp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"* Active\\n** TODO Keep\\n** Archive                                                          :ARCHIVE:\\n*** DONE Finished\\n:PROPERTIES:\\n:ARCHIVE_TIME: <time>\\n:END:\\nBody\\n\"""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r#"(progn
  (require 'org)
  (require 'org-archive)
  (with-temp-buffer
    (org-mode)
    (insert "* Active\n")
    (insert "** DONE Finished\n")
    (insert "Body\n")
    (insert "** TODO Keep\n")
    (goto-char (point-min))
    (search-forward "Finished")
    (beginning-of-line)
    (org-archive-to-archive-sibling)
    (replace-regexp-in-string
     ":ARCHIVE_TIME: .*"
     ":ARCHIVE_TIME: <time>"
     (buffer-substring-no-properties (point-min) (point-max)))))"#,
        expect,
    );
}

#[test]
fn org_refile_file_backed_subtree_to_target_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"* Inbox\\n* Projects\\n** Target\\n*** TODO Task\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-refile)
  (let ((file (make-temp-file "org-refile" nil ".org"
                              "* Inbox\n** TODO Task\n* Projects\n** Target\n")))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (org-mode)
          (goto-char (point-min))
          (search-forward "Task")
          (beginning-of-line)
          (let ((target-pos (save-excursion
                              (goto-char (point-min))
                              (search-forward "Target")
                              (line-beginning-position))))
            (org-refile nil nil (list "Target" file nil target-pos)))
          (save-buffer)
          (buffer-substring-no-properties (point-min) (point-max)))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file)))))"#,
        expect,
    );
}

#[test]
fn org_id_file_location_lookup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t 1 \"* Target\\n:PROPERTIES:\\n:ID: fixed-id-1\\n:END:\\nBody\\n* Other\\n\" \"org\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-id)
  (let* ((file (make-temp-file "org-id" nil ".org"
                               "* Target
:PROPERTIES:
:ID: fixed-id-1
:END:
Body
* Other
"))
         (org-id-locations-file (make-temp-file "org-id-loc"))
         (org-id-track-globally t))
    (unwind-protect
        (progn
          (org-id-update-id-locations (list file) t)
          (let ((marker (org-id-find "fixed-id-1" t)))
            (list (markerp marker)
                  (and marker (marker-position marker))
                  (and marker
                       (with-current-buffer (marker-buffer marker)
                         (buffer-substring-no-properties (point-min) (point-max))))
                  (file-name-extension (gethash "fixed-id-1" org-id-locations)))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (when (file-exists-p file) (delete-file file))
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))"#,
        expect,
    );
}

#[test]
fn org_citation_parse_styles_and_keys_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"t\" nil (\"doe2020\" \"roe2021\")) (nil nil (\"solo\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (with-temp-buffer
    (org-mode)
    (insert "#+cite_export: basic author-year\n")
    (insert "Text [cite/t:@doe2020; see @roe2021 p. 4] and [cite:@solo].\n")
    (insert "#+bibliography: refs.bib\n")
    (let ((out nil))
      (org-element-map (org-element-parse-buffer) 'citation
        (lambda (citation)
          (push (list (org-element-property :style citation)
                      (org-element-property :prefix citation)
                      (org-cite-get-references citation t))
                out)))
      (nreverse out))))"##,
        expect,
    );
}

#[test]
fn org_table_remote_reference_formula_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+NAME: source\\n| item | value |\\n|------+-------|\\n| a    |     2 |\\n| b    |     3 |\\n\\n#+NAME: summary\\n| total | 6 |\\n#+TBLFM: @1$2=remote(source,@>$2)+remote(source,@>$2)\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: source\n")
    (insert "| item | value |\n")
    (insert "|------+-------|\n")
    (insert "| a | 2 |\n")
    (insert "| b | 3 |\n\n")
    (insert "#+NAME: summary\n")
    (insert "| total |  |\n")
    (insert "#+TBLFM: @1$2=remote(source,@>$2)+remote(source,@>$2)\n")
    (goto-char (point-min))
    (search-forward "summary")
    (org-table-recalculate-buffer-tables)
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_capture_string_file_headline_template_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"* Inbox\\n** TODO Captured task\\n:PROPERTIES:\\n:Source: \\n:END:\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-capture)
  (let* ((file (make-temp-file "org-capture" nil ".org" "* Inbox\n"))
         (org-capture-templates
          `(("t" "Todo" entry (file+headline ,file "Inbox")
             "** TODO %i\n:PROPERTIES:\n:Source: %a\n:END:\n"
             :empty-lines 0))))
    (unwind-protect
        (progn
          (org-capture-string "Captured task" "t")
          (org-capture-finalize)
          (with-temp-buffer
            (insert-file-contents file)
            (buffer-string)))
      (when (get-buffer "CAPTURE-org-capture")
        (kill-buffer "CAPTURE-org-capture"))
      (when (file-exists-p file) (delete-file file)))))"#,
        expect,
    );
}

#[test]
fn org_duration_parse_format_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((90.0 135.0 4565.0 90.0) (\"1:30\" \"2:15\" \"3d 4:05\" \"1:30\") (0 0 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-duration)
  (let* ((durations '("1:30" "2h 15min" "3d 4:05" "1.5h"))
         (minutes (mapcar #'org-duration-to-minutes durations))
         (roundtrip (mapcar #'org-duration-from-minutes minutes)))
    (list minutes
          roundtrip
          (mapcar #'org-duration-p durations))))"#,
        expect,
    );
}

#[test]
fn org_datetree_multiple_dates_ordering_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"\\n* 2026\\n** 2026-04 April\\n*** 2026-04-01 Wednesday\\n\\n**** Earlier\\n** 2026-05 May\\n*** 2026-05-27 Wednesday\\n**** First\\n*** 2026-05-28 Thursday\\n**** Second\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-datetree)
  (with-temp-buffer
    (org-mode)
    (org-datetree-file-entry-under "* First" '(5 27 2026))
    (org-datetree-file-entry-under "* Second" '(5 28 2026))
    (org-datetree-file-entry-under "* Earlier" '(4 1 2026))
    (buffer-substring-no-properties (point-min) (point-max))))"#,
        expect,
    );
}

#[test]
fn org_macro_collect_and_replace_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+MACRO: greet Hello, $1!\\n#+MACRO: wrap /$1/\\n#+MACRO: twice $1 and $1\\nText Hello, Ada! /bold/ x and x.\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-macro)
  (with-temp-buffer
    (org-mode)
    (insert "#+MACRO: greet Hello, $1!\n")
    (insert "#+MACRO: wrap /$1/\n")
    (insert "#+MACRO: twice $1 and $1\n")
    (insert "Text {{{greet(Ada)}}} {{{wrap(bold)}}} {{{twice(x)}}}.\n")
    (org-macro-replace-all (org-macro--collect-macros))
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_list_struct_to_lisp_and_back_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 nil (1)) (40 nil (2))) (ordered (\"[X] first\" (unordered (\"child a\") (\"child b\"))) (\"[ ] second\")) \"1. [X] first\\n  - child a\\n  - child b\\n1. [ ] second\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "1. [X] first\n")
    (insert "   - child a\n")
    (insert "   - child b\n")
    (insert "2. [ ] second\n")
    (goto-char (point-min))
    (let* ((struct (org-list-struct))
           (parents (org-list-parents-alist struct))
           (prevs (org-list-prevs-alist struct))
           (items (mapcar (lambda (item)
                            (list item
                                  (org-list-get-parent item struct parents)
                                  (org-list-get-item-number item struct prevs parents)))
                          (org-list-get-all-items (point-min) struct prevs)))
           (as-lisp (org-list-to-lisp)))
      (list items as-lisp (org-list-to-org as-lisp)))))"#,
        expect,
    );
}

#[test]
fn org_markdown_export_markup_lists_and_links_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t \"\\n\\n# Head\\n\\nParagraph with **bold**, *italic*, `code`, and [link](https://example.org).\\n\\n-   [X] done\\n-   [ ] todo\\n\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-md)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Demo\n")
    (insert "* Head\n")
    (insert "Paragraph with *bold*, /italic/, =code=, and [[https://example.org][link]].\n")
    (insert "- [X] done\n")
    (insert "- [ ] todo\n")
    (let* ((org-export-with-toc nil)
           (md (org-export-as 'md nil nil t nil)))
      (list (not (null (string-match-p "# Head" md)))
            (not (null (string-match-p "\\*\\*bold\\*\\*" md)))
            (not (null (string-match-p "\\*italic\\*" md)))
            (not (null (string-match-p "`code`" md)))
            (not (null (string-match-p "\\[link\\](https://example.org)" md)))
            (not (null (string-match-p "\\[X\\] done" md)))
            md))))"##,
        expect,
    );
}

#[test]
fn org_lint_selected_checker_reports_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((25 \"Missing language in source block\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-lint)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: dup\n")
    (insert "#+NAME: dup\n")
    (insert "#+begin_src\nmissing language\n#+end_src\n")
    (insert "[fn:missing]\n")
    (let* ((ast (org-element-parse-buffer))
           (reports (append (org-lint-duplicate-name ast)
                            (org-lint-missing-language-in-src-block ast)
                            (org-lint-undefined-footnote-reference ast))))
      (mapcar (lambda (report)
                (list (car report) (nth 1 report)))
              reports))))"##,
        expect,
    );
}

#[test]
fn org_table_transpose_after_alignment_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"| Name   | Qty |\\n|--------+-----|\\n| banana |  10 |\\n| apple  |   2 |\\n| cherry |   5 |\\n\" \"| Name | banana | apple | cherry |\\n| Qty  |     10 |     2 |      5 |\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Qty |\n")
    (insert "|------+-----|\n")
    (insert "| banana | 10 |\n")
    (insert "| apple | 2 |\n")
    (insert "| cherry | 5 |\n")
    (goto-char (point-min))
    (org-table-align)
    (let ((aligned (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (org-table-transpose-table-at-point)
      (list aligned
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_src_edit_buffer_writeback_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+begin_src emacs-lisp\\n  (+ 3 4)\\n  (message \\\"done\\\")\\n#+end_src\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (org-mode)
    (insert "#+begin_src emacs-lisp\n")
    (insert "(+ 1 2)\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (search-forward "(+ 1 2)")
    (org-edit-src-code)
    (erase-buffer)
    (insert "(+ 3 4)\n")
    (insert "(message \"done\")\n")
    (org-edit-src-exit)
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_insert_demote_and_detect_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 5 \"***** Inline body\\n\\n***** END\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-inlinetask)
  (with-temp-buffer
    (org-mode)
    (let ((org-inlinetask-min-level 4))
      (org-inlinetask-insert-task t)
      (insert "Inline body\n")
      (org-inlinetask-goto-beginning)
      (org-inlinetask-demote)
      (org-inlinetask-goto-beginning)
      (list (org-inlinetask-at-task-p)
            (org-inlinetask-get-task-level)
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_entities_lookup_latex_html_utf8_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"\\\\alpha\" \"&alpha;\" \"α\") (\"nbsp\" \"~\" \"&nbsp;\" \"\u{a0}\") (\"copy\" \"\\\\textcopyright{}\" \"&copy;\" \"©\") (\"rightarrow\" \"\\\\rightarrow\" \"&rarr;\" \"→\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-entities)
  (mapcar (lambda (name)
            (let ((entry (assoc name org-entities)))
              (list name (nth 1 entry) (nth 3 entry) (nth 6 entry))))
          '("alpha" "nbsp" "copy" "rightarrow")))"#,
        expect,
    );
}

#[test]
fn org_ascii_export_links_code_and_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"1 Head\\n══════\\n\\n  Paragraph with [Example] and `code'.\\n  ━━━━━━\\n   A  B \\n   1  2 \\n  ━━━━━━\\n\\n\\n[Example] <https://example.org>\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-ascii)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Plain\n")
    (insert "* Head\n")
    (insert "Paragraph with [[https://example.org][Example]] and =code=.\n")
    (insert "| A | B |\n")
    (insert "| 1 | 2 |\n")
    (let ((org-export-with-toc nil)
          (org-ascii-charset 'utf-8))
      (org-export-as 'ascii nil nil t nil))))"##,
        expect,
    );
}

#[test]
fn org_fold_hide_show_subtree_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (2 nil \"* Parent\\nbody\\n** Child\\nchild\\n* Next\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Parent\nbody\n** Child\nchild\n* Next\n")
    (goto-char (point-min))
    (org-fold-hide-subtree)
    (let ((hidden-after-hide
           (invisible-p (save-excursion (search-forward "body") (point)))))
      (org-fold-show-subtree)
      (let ((hidden-after-show
             (invisible-p (save-excursion
                            (goto-char (point-min))
                            (search-forward "body")
                            (point)))))
        (list hidden-after-hide
              hidden-after-show
              (buffer-substring-no-properties (point-min) (point-max)))))))"#,
        expect,
    );
}

#[test]
fn org_publish_project_html_file_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (t t t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((base (make-temp-file "org-pub" t))
         (pub (make-temp-file "org-pub-out" t))
         (src (expand-file-name "index.org" base))
         (org-publish-project-alist
          `(("probe"
             :base-directory ,base
             :publishing-directory ,pub
             :publishing-function org-html-publish-to-html
             :with-toc nil))))
    (unwind-protect
        (progn
          (with-temp-file src
            (insert "#+TITLE: Publish Probe\n")
            (insert "* Head\n")
            (insert "Body with [[https://example.org][link]].\n"))
          (org-publish-project "probe" t)
          (let ((html (expand-file-name "index.html" pub)))
            (list (file-exists-p html)
                  (with-temp-buffer
                    (insert-file-contents html)
                    (list (not (null (string-match-p "Publish Probe" (buffer-string))))
                          (not (null (string-match-p "Head" (buffer-string))))
                          (not (null (string-match-p "https://example.org" (buffer-string)))))))))
      (delete-directory base t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_habit_detection_with_repeater_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"Habit\" t) (\"Plain\" nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-habit)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Habit\n")
    (insert "SCHEDULED: <2026-05-27 Wed .+1d/3d>\n")
    (insert ":PROPERTIES:\n:STYLE: habit\n:END:\n")
    (insert "* TODO Plain\n")
    (insert "SCHEDULED: <2026-05-27 Wed>\n")
    (goto-char (point-min))
    (let ((out nil))
      (while (re-search-forward org-heading-regexp nil t)
        (beginning-of-line)
        (push (list (org-get-heading t t t t)
                    (org-is-habit-p))
              out)
        (forward-line 1))
      (nreverse out))))"#,
        expect,
    );
}

#[test]
fn org_ordered_entry_blocking_and_inheritance_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"t\" ((\"CATEGORY\" . \"???\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-enforce-todo-dependencies t))
      (org-mode)
      (insert "* TODO A\n")
      (insert ":PROPERTIES:\n:ORDERED: t\n:END:\n")
      (insert "** TODO first\n")
      (insert "** TODO second\n")
      (goto-char (point-min))
      (search-forward "second")
      (beginning-of-line)
      (list (org-entry-blocked-p)
            (org-entry-get-with-inheritance "ORDERED")
            (org-entry-properties nil 'standard)))))"#,
        expect,
    );
}

#[test]
fn org_priority_tags_properties_todo_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"TODO\") nil 1000 \"2:00\" (\"work\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO A [#B] :work:\n")
    (insert "SCHEDULED: <2026-05-27 Wed> DEADLINE: <2026-05-28 Thu>\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
    (goto-char (point-min))
    (list (org-entry-is-todo-p)
          (org-entry-is-done-p)
          (org-get-priority (thing-at-point 'line t))
          (org-entry-get nil "Effort")
          (org-get-tags))))"#,
        expect,
    );
}

#[test]
fn org_tempo_source_template_expansion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK \"#+begin_src \\n\\n#+end_src\"""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-tempo)
  (with-temp-buffer
    (org-mode)
    (insert "<s")
    (org-tempo-complete-tag)
    (buffer-substring-no-properties (point-min) (point-max))))"#,
        expect,
    );
}

#[test]
fn org_custom_link_follow_and_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"ticket\" \"ABC-123\" (\"ABC-123\" nil) \"<p>\\nTICKET:ABC-123:Bug:html</p>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'ol)
  (with-temp-buffer
    (org-mode)
    (org-link-set-parameters
     "ticket"
     :follow (lambda (path arg) (list path arg))
     :export (lambda (path desc backend info)
               (format "TICKET:%s:%s:%s" path desc backend)))
    (insert "See [[ticket:ABC-123][Bug]].\n")
    (let ((link (org-element-map (org-element-parse-buffer) 'link
                  (lambda (candidate) candidate)
                  nil t)))
      (list (org-element-property :type link)
            (org-element-property :path link)
            (org-link-open-from-string "[[ticket:ABC-123]]" nil)
            (org-export-string-as "[[ticket:ABC-123][Bug]]" 'html t)))))"#,
        expect,
    );
}

#[test]
fn org_timer_conversion_and_region_shift_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (3723 \"1:02:03\" (\"0:00:05\" \"0:01:02\" \"1:02:03\") \"00:00:15\\n01:02:08\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-timer)
  (with-temp-buffer
    (insert "00:00:10\n01:02:03\n")
    (org-timer-change-times-in-region (point-min) (point-max) "0:00:05")
    (list (org-timer-hms-to-secs "1:02:03")
          (org-timer-secs-to-hms 3723)
          (mapcar (lambda (s) (org-timer-fix-incomplete s))
                  '("5" "1:02" "1:02:03"))
          (buffer-substring-no-properties (point-min) (point-max)))))"#,
        expect,
    );
}

#[test]
fn org_protocol_uri_query_parameter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"org-protocol://capture?template=t&url=https%3A%2F%2Fexample.org%2Fa%3Fb%3D1&title=Hello%20World\" (\"capture:\" \"x\" \"y\" \"z\") (:template \"t\" :url \"https%3A%2F%2Fexample.org\" :title \"Hello%20World\") (:template \"template=t&url=https://example.org&title=Hello World\" :url nil :title nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-protocol)
  (list
   (org-protocol-sanitize-uri
    "org-protocol://capture?template=t&url=https%3A%2F%2Fexample.org%2Fa%3Fb%3D1&title=Hello%20World")
   (org-protocol-split-data "capture://x/y/z" t)
   (org-protocol-convert-query-to-plist
    "template=t&url=https%3A%2F%2Fexample.org&title=Hello%20World")
   (org-protocol-parse-parameters
    "template=t&url=https%3A%2F%2Fexample.org&title=Hello%20World"
    nil
    '(:template :url :title))))"#,
        expect,
    );
}

#[test]
fn org_feed_rss_atom_parse_entry_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((:guid \"one-guid\" :item-full-text \"<guid>one-guid</guid><title>One</title><link>https://example.org/1</link><description>Body</description><pubDate>Wed, 27 May 2026 10:00:00 GMT</pubDate>\" :title \"One\" :link \"https://example.org/1\" :description \"Body\" :pubDate \"Wed, 27 May 2026 10:00:00 GMT\" :guid-permalink t)) ((:guid \"tag:example.org,2026:2\" :item-full-text \"(entry nil (title nil \\\"Two\\\") (id nil \\\"tag:example.org,2026:2\\\") (updated nil \\\"2026-05-27T11:00:00Z\\\") (link ((href . \\\"https://example.org/2\\\"))) (content ((type . \\\"text\\\")) \\\"Atom body\\\"))\" :link \"https://example.org/2\" :title \"Two\" :description \"Atom body\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-feed)
  (let ((rss-buffer (generate-new-buffer " *rss*"))
        (atom-buffer (generate-new-buffer " *atom*")))
    (unwind-protect
        (progn
          (with-current-buffer rss-buffer
            (insert "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Feed</title><item><guid>one-guid</guid><title>One</title><link>https://example.org/1</link><description>Body</description><pubDate>Wed, 27 May 2026 10:00:00 GMT</pubDate></item></channel></rss>"))
          (with-current-buffer atom-buffer
            (insert "<?xml version=\"1.0\"?><feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Atom</title><entry><title>Two</title><id>tag:example.org,2026:2</id><updated>2026-05-27T11:00:00Z</updated><link href=\"https://example.org/2\"/><content type=\"text\">Atom body</content></entry></feed>"))
          (list (mapcar #'org-feed-parse-rss-entry
                        (org-feed-parse-rss-feed rss-buffer))
                (mapcar #'org-feed-parse-atom-entry
                        (org-feed-parse-atom-feed atom-buffer))))
      (kill-buffer rss-buffer)
      (kill-buffer atom-buffer))))"##,
        expect,
    );
}

#[test]
fn org_mobile_escape_body_tag_compare_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"A%3AB%2FC\" nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-mobile)
  (list (org-mobile-escape-olp "A:B/C")
        (org-mobile-tags-same-p '("a" "b") '("b" "a"))
        (org-mobile-tags-same-p '("a" "b") '("a" "c"))
        (org-mobile-bodies-same-p "  A \n B  " "A\nB")
        (org-mobile-bodies-same-p "A\nB" "A\n C")))"#,
        expect,
    );
}

#[test]
fn org_plot_collect_options_table_metadata_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Demo Plot\" 1 (2 3) nil lines (\"grid\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (with-temp-buffer
    (org-mode)
    (insert "#+PLOT: title:\"Demo Plot\" ind:1 deps:(2 3) type:2d with:lines set:\"grid\"\n")
    (insert "| X | A | B |\n")
    (insert "|---+---+---|\n")
    (insert "| 1 | 2 | 3 |\n")
    (goto-char (point-min))
    (let ((opts (org-plot/collect-options '(:include t))))
      (list (plist-get opts :title)
            (plist-get opts :ind)
            (plist-get opts :deps)
            (plist-get opts :type)
            (plist-get opts :with)
            (plist-get opts :set)
            (plist-get opts :include)))))"##,
        expect,
    );
}

#[test]
fn org_latex_export_markup_math_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t \"\\\\section{Head}\\n\\\\label{sec:org-id}\\nText with \\\\textbf{bold}, \\\\emph{italic}, \\\\texttt{code}, \\\\href{https://example.org}{Example}, and \\\\(x^2\\\\).\\n\\\\begin{center}\\n\\\\begin{tabular}{rr}\\nA & B\\\\\\\\\\n1 & 2\\\\\\\\\\n\\\\end{tabular}\\n\\\\end{center}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-latex)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Latex Probe\n")
    (insert "* Head\n")
    (insert "Text with *bold*, /italic/, =code=, [[https://example.org][Example]], and $x^2$.\n")
    (insert "| A | B |\n")
    (insert "| 1 | 2 |\n")
    (let* ((org-export-with-toc nil)
           (latex (org-export-as 'latex nil nil t nil)))
      (list (not (null (string-match-p "\\\\section" latex)))
            (not (null (string-match-p "\\\\textbf{bold}" latex)))
            (not (null (string-match-p "\\\\emph{italic}" latex)))
            (not (null (string-match-p "\\\\texttt{code}" latex)))
            (not (null (string-match-p "\\\\href{https://example.org}{Example}" latex)))
            (not (null (string-match-p "tabular" latex)))
            (replace-regexp-in-string
             "\\\\label{sec:org[[:alnum:]]+}"
             "\\\\label{sec:org-id}"
             latex)))))"##,
        expect,
    );
}

#[test]
fn org_org_export_todo_schedule_link_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t nil t \"* TODO Head                                                             :tag:\\nBody with *bold* and [[https://example.org][link]].\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-org)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Org Export\n")
    (insert "* TODO Head :tag:\n")
    (insert "SCHEDULED: <2026-05-27 Wed>\n")
    (insert "Body with *bold* and [[https://example.org][link]].\n")
    (let* ((org-export-with-toc nil)
           (out (org-export-as 'org nil nil t nil)))
      (list (not (null (string-match-p "#\\+TITLE: Org Export" out)))
            (not (null (string-match-p "\\* TODO Head" out)))
            (not (null (string-match-p "SCHEDULED:" out)))
            (not (null (string-match-p "\\[\\[https://example.org\\]\\[link\\]\\]" out)))
            out))))"##,
        expect,
    );
}

#[test]
fn org_map_entries_inherited_tags_and_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Alpha\" (\"work\") \"Ada\" \"1:30\") (\"WAIT Child\" (\"urgent\") \"Ada\" \"0:45\")) ((\"Alpha\" \"1:30\") (\"WAIT Child\" \"0:45\") (\"Beta\" \"2:00\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t))
      (org-mode)
      (insert "#+TODO: TODO WAIT | DONE\n")
      (insert "* TODO Alpha :work:\n")
      (insert ":PROPERTIES:\n:Effort: 1:30\n:Owner: Ada\n:END:\n")
      (insert "** WAIT Child :urgent:\n")
      (insert ":PROPERTIES:\n:Effort: 0:45\n:END:\n")
      (insert "* DONE Beta :home:\n")
      (insert ":PROPERTIES:\n:Effort: 2:00\n:END:\n")
      (goto-char (point-min))
      (list
       (org-map-entries
        (lambda ()
          (list (org-get-heading t t t t)
                (org-get-tags nil t)
                (org-entry-get nil "Owner" t)
                (org-entry-get nil "Effort")))
        "+work"
        nil)
       (org-map-entries
        (lambda ()
          (list (org-get-heading t t t t)
                (org-entry-get nil "Effort")))
        "Effort={.+}"
        nil)))))"##,
        expect,
    );
}

#[test]
fn org_columnview_dynamic_block_properties_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK \"#+COLUMNS: %25ITEM %TODO %3PRIORITY %Effort{:} %Owner\\n* TODO Project\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n** TODO Alpha [#A]\\n:PROPERTIES:\\n:Effort: 1:30\\n:END:\\n** WAIT Beta [#C]\\n:PROPERTIES:\\n:Effort: 0:45\\n:Owner: Bob\\n:END:\\n#+BEGIN: columnview :hlines 1 :id local\\n| <25>           |      | <3>      |        |       |\\n| ITEM           | TODO | PRIORITY | Effort | Owner |\\n|----------------+------+----------+--------+-------|\\n| WAIT Beta [#C] |      | C        |   0:45 | Bob   |\\n#+END:\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %Effort{:} %Owner\n")
    (insert "* TODO Project\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
    (insert "** TODO Alpha [#A]\n")
    (insert ":PROPERTIES:\n:Effort: 1:30\n:END:\n")
    (insert "** WAIT Beta [#C]\n")
    (insert ":PROPERTIES:\n:Effort: 0:45\n:Owner: Bob\n:END:\n")
    (insert "#+BEGIN: columnview :hlines 1 :id local\n")
    (insert "#+END:\n")
    (goto-char (point-min))
    (search-forward "#+BEGIN: columnview")
    (org-dblock-update)
    (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn org_sort_child_entries_priority_then_todo_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Parent\\n** DONE A [#A]\\n:PROPERTIES:\\n:Order: 1\\n:END:\\n** WAIT C [#B]\\n:PROPERTIES:\\n:Order: 3\\n:END:\\n** TODO B [#C]\\n:PROPERTIES:\\n:Order: 2\\n:END:\\n\" \"* Parent\\n** TODO B [#C]\\n:PROPERTIES:\\n:Order: 2\\n:END:\\n** WAIT C [#B]\\n:PROPERTIES:\\n:Order: 3\\n:END:\\n** DONE A [#A]\\n:PROPERTIES:\\n:Order: 1\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Parent\n")
    (insert "** TODO B [#C]\n:PROPERTIES:\n:Order: 2\n:END:\n")
    (insert "** DONE A [#A]\n:PROPERTIES:\n:Order: 1\n:END:\n")
    (insert "** WAIT C [#B]\n:PROPERTIES:\n:Order: 3\n:END:\n")
    (goto-char (point-min))
    (org-sort-entries nil ?p)
    (let ((by-priority (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (org-sort-entries nil ?o)
      (list by-priority
            (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_attach_copy_list_and_tag_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (\"payload.txt\") \"payload\" t \"* Node                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: fixed-attach\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-attach-root" t))
         (src (expand-file-name "payload.txt" root))
         (attach-root (expand-file-name "attach" root))
         (default-directory root)
         (org-attach-id-dir attach-root)
         (org-id-method 'org)
         (org-attach-store-link-p nil))
    (unwind-protect
        (progn
          (with-temp-file src (insert "payload"))
          (with-temp-buffer
            (org-mode)
            (insert "* Node\n:PROPERTIES:\n:ID: fixed-attach\n:END:\n")
            (goto-char (point-min))
            (search-forward "Node")
            (let ((org-attach-method 'cp))
              (org-attach-attach src nil 'cp))
            (let* ((dir (org-attach-dir))
                   (files (mapcar #'file-name-nondirectory
                                  (org-attach-file-list dir)))
                   (payload
                    (with-temp-buffer
                      (insert-file-contents (expand-file-name "payload.txt" dir))
                      (buffer-string))))
              (list (file-directory-p dir)
                    files
                    payload
                    (string-prefix-p attach-root dir)
                    (buffer-substring-no-properties (point-min) (point-max))))))
      (delete-directory root t))))"#,
        expect,
    );
}

#[test]
fn org_element_planning_property_timestamp_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable timestamp-summary)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha\n")
    (insert "DEADLINE: <2026-05-28 Thu> SCHEDULED: <2026-05-27 Wed>\n")
    (insert ":PROPERTIES:\n:Effort: 1:30\n:END:\n")
    (insert "* Plain\n")
    (insert "<2026-05-29 Fri 10:00-11:15>\n")
    (let ((out nil)
          (timestamp-summary
           (lambda (timestamp)
             (and timestamp
                  (list (org-element-property :type timestamp)
                        (org-element-property :range-type timestamp)
                        (org-element-property :raw-value timestamp)
                        (org-element-property :year-start timestamp)
                        (org-element-property :month-start timestamp)
                        (org-element-property :day-start timestamp)
                        (org-element-property :hour-start timestamp)
                        (org-element-property :minute-start timestamp)
                        (org-element-property :hour-end timestamp)
                        (org-element-property :minute-end timestamp)))))))
      (org-element-map
          (org-element-parse-buffer)
          '(headline planning timestamp node-property)
        (lambda (element)
          (push
           (list (org-element-type element)
                 (org-element-property :todo-keyword element)
                 (org-element-property :raw-value element)
                 (funcall timestamp-summary
                          (org-element-property :deadline element))
                 (funcall timestamp-summary
                          (org-element-property :scheduled element))
                 (org-element-property :key element)
                 (org-element-property :value element)
                 (funcall timestamp-summary element))
           out)))
      (nreverse out))))"#,
        expect,
    );
}
