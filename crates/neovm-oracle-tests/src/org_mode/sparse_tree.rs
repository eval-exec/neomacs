use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_occur_stacked_sparse_tree_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-fold-show-context-detail '((occur-tree . lineage)))
          (org-highlight-sparse-tree-matches t)
          (org-remove-highlights-with-change nil)
          (org-occur-hook nil))
      (org-mode)
      (insert "* TODO Alpha :work:\nneedle alpha\n** WAIT Child\nchild needle\n")
      (insert "* DONE Beta :home:\nno match\n** TODO Grand\nneedle grand\n")
      (insert "* TODO Gamma :work:\nother text\n")
      (let ((first (org-occur "needle"))
            (second (org-occur "TODO" t
                               (lambda ()
                                 (save-excursion
                                   (org-back-to-heading t)
                                   (member "work" (org-get-tags)))))))
        (list
         first
         second
         (length org-occur-highlights)
         (mapcar (lambda (needle)
                   (let ((pos (save-excursion
                                (goto-char (point-min))
                                (search-forward needle)
                                (point))))
                     (list needle (not (null (org-invisible-p pos))))))
                 '("Alpha" "needle alpha" "Child" "child needle"
                   "Beta" "Grand" "needle grand" "Gamma" "other text"))
         (mapcar (lambda (ov)
                   (list (overlay-start ov)
                         (overlay-end ov)
                         (overlay-get ov 'org-type)))
                 org-occur-highlights)
         (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_tags_sparse_tree_property_archive_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Project\" nil) (\"Active\" nil) (\"Closed\" t) (\"Archived\" nil) (\"Old\" t) (\"Other\" nil) (\"Home\" nil)) \"* Project :work:\\n** TODO Active :urgent:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n** DONE Closed :urgent:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n* Archived :work:ARCHIVE:\\n** TODO Old :urgent:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n* Other :home:\\n** TODO Home :urgent:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t)
          (org-sparse-tree-open-archived-trees nil))
      (org-mode)
      (insert "* Project :work:\n")
      (insert "** TODO Active :urgent:\n:PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** DONE Closed :urgent:\n:PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "* Archived :work:ARCHIVE:\n")
      (insert "** TODO Old :urgent:\n:PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "* Other :home:\n")
      (insert "** TODO Home :urgent:\n:PROPERTIES:\n:Owner: Ada\n:END:\n")
      (org-match-sparse-tree nil "+work+urgent+TODO=\"TODO\"+Owner=\"Ada\"")
      (list
       (mapcar (lambda (needle)
                 (let ((pos (save-excursion
                              (goto-char (point-min))
                              (search-forward needle)
                              (point))))
                   (list needle (not (null (org-invisible-p pos))))))
               '("Project" "Active" "Closed" "Archived" "Old" "Other" "Home"))
       (buffer-substring-no-properties (point-min) (point-max))))))"#,
        expect,
    );
}

#[test]
fn org_occur_highlight_removal_after_buffer_change_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'org)
  (with-temp-buffer
    (let ((org-highlight-sparse-tree-matches t)
          (org-remove-highlights-with-change t)
          (org-occur-hook nil))
      (org-mode)
      (insert "* One\nneedle one\n* Two\nneedle two\n")
      (let ((count (org-occur "needle"))
            (before (mapcar #'overlay-buffer org-occur-highlights)))
        (goto-char (point-max))
        (insert "\nchanged\n")
        (let ((after (mapcar #'overlay-buffer org-occur-highlights)))
          (list count
                (length before)
                before
                (length org-occur-highlights)
                after
                org-occur-parameters
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"#,
        expect,
    );
}

#[test]
fn org_sparse_todo_occur_navigation_highlight_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable snapshot)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-highlight-sparse-tree-matches t)
          (org-remove-highlights-with-change nil)
          (org-occur-hook nil)
          (org-sparse-tree-open-archived-trees nil)
          (org-use-tag-inheritance t))
      (org-mode)
      (insert "#+TODO: TODO NEXT WAIT | DONE CANCELED\n")
      (insert "* TODO Project :work:\nneedle root\n")
      (insert "** NEXT Alpha :urgent:\nalpha needle\n")
      (insert "*** WAIT Alpha child :blocked:\nchild needle\n")
      (insert "** DONE Finished :urgent:\nfinished needle\n")
      (insert "* TODO Archived :work:ARCHIVE:\narchived needle\n")
      (insert "** NEXT Old :urgent:\nold needle\n")
      (insert "* WAIT Home :home:\nhome needle\n")
      (let (states)
        (let ((snapshot
               (lambda (label)
                 (list label
                       (mapcar (lambda (needle)
                                 (save-excursion
                                   (goto-char (point-min))
                                   (search-forward needle)
                                   (list needle
                                         (line-number-at-pos)
                                         (not (null
                                               (org-invisible-p
                                                (point))))
                                         (org-element-type
                                          (org-element-at-point)))))
                               '("Project" "needle root" "Alpha"
                                 "alpha needle" "Alpha child"
                                 "child needle" "Finished"
                                 "finished needle" "Archived"
                                 "archived needle" "Old" "old needle"
                                 "Home" "home needle"))
                       (mapcar (lambda (ov)
                                 (list (overlay-start ov)
                                       (overlay-end ov)
                                       (overlay-get ov 'org-type)))
                               org-occur-highlights)
                       (buffer-substring-no-properties
                        (point-min) (point-max)))))))
          (push (funcall snapshot 'initial) states)
          (org-show-todo-tree nil)
          (push (funcall snapshot 'todo-tree) states)
          (let ((occur-count (org-occur "needle" t
                                        (lambda ()
                                          (save-excursion
                                            (org-back-to-heading t)
                                            (member "urgent"
                                                    (org-get-tags)))))))
            (push (funcall snapshot 'occur-urgent) states)
            (let (moves)
              (dotimes (i 4)
                (condition-case err
                    (progn
                      (org-occur-next-match 1)
                      (push (list i
                                  (line-number-at-pos)
                                  (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))
                                  (org-element-type
                                   (org-element-at-point)))
                            moves))
                  (error (push (list i (cons (car err) (cdr err)))
                               moves))))
              (goto-char (point-min))
              (search-forward "Alpha child")
              (beginning-of-line)
              (org-todo "DONE")
              (insert "edited ")
              (push (funcall snapshot 'after-edit) states)
              (let ((highlight-count-before-clear
                     (length org-occur-highlights)))
                (org-remove-occur-highlights)
                (push (funcall snapshot 'after-clear) states)
                (list occur-count
                      (nreverse moves)
                      highlight-count-before-clear
                      (nreverse states)
                      (length org-occur-highlights)
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_sparse_date_type_range_visibility_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-highlight-sparse-tree-matches t)
          (org-remove-highlights-with-change nil)
          (org-occur-hook nil)
          (org-sparse-tree-open-archived-trees nil)
          (org-fold-show-context-detail '((occur-tree . ancestors))))
      (org-mode)
      (insert "* TODO Alpha :work:\n")
      (insert "SCHEDULED: <2026-05-25 Mon> DEADLINE: <2026-05-29 Fri>\n")
      (insert "body alpha <2026-05-28 Thu>\n")
      (insert "** DONE Alpha child\n")
      (insert "CLOSED: [2026-05-26 Tue] SCHEDULED: <2026-05-30 Sat>\n")
      (insert "child body [2026-05-24 Sun]\n")
      (insert "* TODO Beta :home:\n")
      (insert "DEADLINE: <2026-05-26 Tue>\n")
      (insert "body beta\n")
      (insert "* DONE Gamma :work:ARCHIVE:\n")
      (insert "CLOSED: [2026-05-27 Wed] SCHEDULED: <2026-05-27 Wed>\n")
      (insert "archived body\n")
      (insert "* TODO Delta\n")
      (insert "SCHEDULED: <2026-06-02 Tue> DEADLINE: <2026-06-03 Wed>\n")
      (insert "delta body\n")
      (let (states)
        (cl-labels
            ((snapshot
              (label result)
              (list
               label
               org-ts-type
               result
               org-occur-parameters
               (mapcar
                (lambda (needle)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward needle)
                    (list needle
                          (line-number-at-pos)
                          (not (null (org-invisible-p (point))))
                          (org-element-type (org-element-at-point)))))
                '("Alpha" "2026-05-25" "2026-05-29"
                  "body alpha" "Alpha child" "2026-05-26"
                  "2026-05-30" "Beta" "Gamma" "2026-05-27"
                  "Delta" "2026-06-02" "2026-06-03"))
               (mapcar
                (lambda (ov)
                  (list (overlay-start ov)
                        (overlay-end ov)
                        (buffer-substring-no-properties
                         (overlay-start ov) (overlay-end ov))
                        (overlay-get ov 'face)
                        (overlay-get ov 'org-type)
                        (overlay-buffer ov)))
                org-occur-highlights))))
          (dolist (spec
                   '((nil range org-check-dates-range
                          ("2026-05-24" "2026-05-31"))
                     (scheduled before org-check-before-date
                                ("2026-05-29"))
                     (deadline after org-check-after-date
                               ("2026-05-27"))
                     (closed range org-check-dates-range
                             ("2026-05-26" "2026-05-28"))
                     (all range org-check-dates-range
                          ("2026-05-24" "2026-05-30"))))
            (org-fold-show-all)
            (org-remove-occur-highlights nil nil t)
            (setq org-ts-type (nth 0 spec))
            (let* ((label (nth 1 spec))
                   (fn (nth 2 spec))
                   (args (nth 3 spec))
                   (result (apply fn args)))
              (push (snapshot label result) states)))
          (let ((highlight-count-before-edit
                 (length org-occur-highlights))
                (params-before-edit org-occur-parameters))
            (goto-char (point-max))
            (insert "\n* TODO Epsilon\nSCHEDULED: <2026-05-28 Thu>\n")
            (let ((after-edit-highlights
                   (mapcar #'overlay-buffer org-occur-highlights)))
              (org-remove-occur-highlights)
              (list (nreverse states)
                    highlight-count-before-edit
                    params-before-edit
                    after-edit-highlights
                    org-occur-highlights
                    org-occur-parameters
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_sparse_tag_todo_deps_planning_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 63 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-agenda)
  (with-temp-buffer
    (let ((org-use-tag-inheritance t)
          (org-agenda-span 'day))
      (org-mode)
      (insert "* TODO Alpha :work:urgent:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert "** TODO Sub A1\n")
      (insert "*** DONE Sub A1a\n")
      (insert "*** TODO Sub A1b\n")
      (insert "** WAIT Sub A2\n")
      (insert "DEADLINE: <2026-05-28 Thu>\n")
      (insert "* Beta :home:\n")
      (insert "** TODO Sub B1 :work:\n")
      (insert "SCHEDULED: <2026-05-26 Tue>\n")
      (insert "** DONE Sub B2\n")
      (insert "CLOSED: [2026-05-27 Wed]\n")
      (insert "* Gamma :work:\n")
      (insert "** TODO Sub G1\n")
      (insert "DEADLINE: <2026-05-27 Wed>\n")
      (insert "* Delta\n")
      (insert "No planning.\n")
      (let ((vis (lambda ()
                   (mapcar
                    (lambda (needle)
                      (save-excursion
                        (goto-char (point-min))
                        (search-forward needle)
                        (list needle
                              (line-number-at-pos)
                              (invisible-p (point))
                              (org-get-tags nil t))))
                    '("Alpha" "Sub A1" "Sub A1a" "Sub A1b" "Sub A2"
                      "Beta" "Sub B1" "Sub B2" "Gamma" "Sub G1"
                      "Delta" "No planning")))))
        ;; Tag sparse tree
        (org-tags-sparse-tree nil "work")
        (let ((tag-vis (funcall vis))
              (tag-buf (buffer-substring-no-properties
                        (point-min) (point-max))))
          (org-remove-occur-highlights)
          ;; TODO sparse tree
          (org-show-todo-tree nil)
          (let ((todo-vis (funcall vis))
                (todo-buf (buffer-substring-no-properties
                           (point-min) (point-max))))
            (org-remove-occur-highlights)
            ;; Planning sparse tree
            (org-check-deadlines 1)
            (let ((deadline-vis (funcall vis))
                  (deadline-buf (buffer-substring-no-properties
                                 (point-min) (point-max))))
              (org-remove-occur-highlights)
              ;; Full buffer after all operations
              (org-fold-show-all)
              (let ((final-buf (buffer-substring-no-properties
                                (point-min) (point-max))))
                (list tag-vis
                      todo-vis
                      deadline-vis
                      final-buf))))))))))"##,
        expect,
    );
}

#[test]
fn org_occur_highlight_count_visibility_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (0 \"3 matches for \\\"important\\\" in buffer:  *temp*\\n      2:Alpha body with important keyword.\\n      6:Sub B body with important note.\\n     10:Sub C body with another important line.\\n\" ((\"Alpha\" 1 nil (\"work\")) (\"Sub A\" 3 nil nil) (\"Sub B\" 5 nil (\"urgent\")) (\"Beta\" 7 nil (\"home\")) (\"Sub C\" 9 nil nil) (\"Gamma\" 11 nil nil)) ((\"Alpha\" 1 nil (\"work\")) (\"Sub A\" 3 nil nil) (\"Sub B\" 5 nil (\"urgent\")) (\"Beta\" 7 nil (\"home\")) (\"Sub C\" 9 nil nil) (\"Gamma\" 11 nil nil)) 0 \"* TODO Alpha :work:\\nAlpha body with important keyword.\\n** DONE Sub A\\nSub A body.\\n** TODO Sub B :urgent:\\nSub B body with important note.\\n* Beta :home:\\nBeta body.\\n** TODO Sub C\\nSub C body with another important line.\\n* DONE Gamma\\nGamma body with keyword inside.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\n")
    (insert "Alpha body with important keyword.\n")
    (insert "** DONE Sub A\n")
    (insert "Sub A body.\n")
    (insert "** TODO Sub B :urgent:\n")
    (insert "Sub B body with important note.\n")
    (insert "* Beta :home:\n")
    (insert "Beta body.\n")
    (insert "** TODO Sub C\n")
    (insert "Sub C body with another important line.\n")
    (insert "* DONE Gamma\n")
    (insert "Gamma body with keyword inside.\n")
    ;; Run occur for "important"
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward needle)
                      (list needle
                            (line-number-at-pos)
                            (invisible-p (point))
                            (org-get-tags nil t))))
                  '("Alpha" "Sub A" "Sub B" "Beta" "Sub C" "Gamma")))))
      (goto-char (point-min))
      (occur "important")
      (let ((occur-buf (buffer-name (other-buffer (current-buffer) t)))
            (occur-count (length org-occur-highlights))
            (occur-text
             (when (get-buffer "*Occur*")
               (with-current-buffer "*Occur*"
                 (buffer-substring-no-properties
                  (point-min) (point-max))))))
        (let ((after-occur-vis (funcall vis)))
          ;; Remove highlights
          (org-remove-occur-highlights)
          (let ((after-remove-vis (funcall vis))
                (highlights-after-remove (length org-occur-highlights)))
            (list occur-count
                  occur-text
                  after-occur-vis
                  after-remove-vis
                  highlights-after-remove
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_sparse_tree_tag_todo_match_edit_occur_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-occur\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-occur)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\nBody alpha.\n\n")
    (insert "** DONE Beta :home:\nBody beta.\n\n")
    (insert "*** TODO Gamma :work:urgent:\nBody gamma.\n\n")
    (insert "** WAIT Delta :home:\nBody delta.\n\n")
    (insert "* DONE Epsilon :work:\nBody epsilon.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (pos)
                    (save-excursion
                      (goto-char pos)
                      (list (buffer-substring-no-properties
                             (line-beginning-position) (line-end-position))
                            (invisible-p pos))))
                  (let ((positions nil))
                    (goto-char (point-min))
                    (while (not (eobp))
                      (push (point) positions)
                      (forward-line 1))
                    (nreverse positions)))))))
      ;; Sparse tree for TODO
      (let ((before-vis (funcall vis)))
        (org-match-sparse-tree nil "TODO")
        (let ((after-match-vis (funcall vis)))
          ;; Edit: add tag to Gamma
          (goto-char (point-min))
          (search-forward "Gamma")
          (org-toggle-tag "extra" 'on)
          ;; Occur for "Body"
          (goto-char (point-min))
          (occur "Body")
          (let ((occur-count (length org-occur-highlights))
                (occur-text
                 (when (get-buffer "*Occur*")
                   (with-current-buffer "*Occur*"
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))
            ;; Remove highlights
            (org-remove-occur-highlights)
            (list before-vis
                  after-match-vis
                  occur-count
                  occur-text
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_sparse_tree_todo_tag_edit_resparse_occur_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-occur\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-occur)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\nBody alpha.\n\n")
    (insert "** DONE Beta :home:\nBody beta.\n\n")
    (insert "*** TODO Gamma :work:\nBody gamma.\n\n")
    (insert "** WAIT Delta :home:\nBody delta.\n\n")
    (insert "* DONE Epsilon :work:\nBody epsilon.\n\n")
    (insert "* TODO Zeta :work:\nBody zeta.\n\n")
    (let ((vis (lambda ()
                 (mapcar
                  (lambda (pos)
                    (save-excursion
                      (goto-char pos)
                      (list (buffer-substring-no-properties
                             (line-beginning-position) (line-end-position))
                            (invisible-p pos))))
                  (let ((positions nil))
                    (goto-char (point-min))
                    (while (not (eobp))
                      (push (point) positions)
                      (forward-line 1))
                    (nreverse positions)))))))
      ;; Sparse tree for TODO
      (let ((before-vis (funcall vis)))
        (org-match-sparse-tree nil "TODO")
        (let ((after-todo-vis (funcall vis)))
          ;; Edit: change Zeta to DONE
          (goto-char (point-min))
          (search-forward "TODO Zeta")
          (replace-match "DONE Zeta")
          ;; Re-sparse
          (org-match-sparse-tree nil "TODO")
          (let ((after-resparse-vis (funcall vis)))
            ;; Sparse by tag
            (org-tags-view nil "work")
            (let ((tag-vis (funcall vis)))
              ;; Occur for "Body"
              (goto-char (point-min))
              (occur "Body")
              (let ((occur-count (length org-occur-highlights)))
                (org-remove-occur-highlights)
                (list before-vis
                      after-todo-vis
                      after-resparse-vis
                      tag-vis
                      occur-count
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))))"##,
        expect,
    );
}
