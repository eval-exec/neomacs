use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_inlinetask_region_insert_promote_demote_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 5)
          (org-inlinetask-default-state "TODO")
          (transient-mark-mode t))
      (org-mode)
      (insert "* Parent\n")
      (insert "Before text\n")
      (insert "Body line one\nBody line two\n")
      (insert "After text\n")
      (goto-char (point-min))
      (search-forward "Body line one")
      (beginning-of-line)
      (push-mark (point) nil t)
      (search-forward "Body line two")
      (end-of-line)
      (org-inlinetask-insert-task nil)
      (let ((after-insert
             (buffer-substring-no-properties (point-min) (point-max)))
            begin-pos end-pos)
        (org-inlinetask-goto-beginning)
        (setq begin-pos (point))
        (org-inlinetask-demote)
        (org-inlinetask-goto-beginning)
        (org-inlinetask-promote)
        (org-inlinetask-goto-end)
        (setq end-pos (point))
        (list after-insert
              begin-pos
              end-pos
              (org-inlinetask-at-task-p)
              (org-inlinetask-get-task-level)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_element_export_archive_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"TODO\" \"Inline one\" (\"tag\") 4 68)) t nil nil \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> Project</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nPlain before.\\n</p>\\n<div class=\\\"inlinetask\\\">\\n<b><span class=\\\"todo TODO\\\">TODO</span> Inline one&nbsp;&nbsp;&nbsp<span class=\\\"tag\\\"><span class=\\\"tag\\\">tag</span></span></b><br />\\n<p>\\nInline body with <a href=\\\"https://example.org\\\">link</a>.\\n</p>\\n</div>\\n<p>\\nPlain after.\\n</p>\\n</div>\\n</div>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (require 'ox-html)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 4)
          (org-export-with-toc nil))
      (org-mode)
      (insert "#+TITLE: Inline\n")
      (insert "* Project\n")
      (insert "Plain before.\n")
      (insert "**** TODO Inline one :tag:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert ":PROPERTIES:\n:Effort: 0:20\n:END:\n")
      (insert "Inline body with [[https://example.org][link]].\n")
      (insert "**** END\n")
      (insert "Plain after.\n")
      (let* ((tree (org-element-parse-buffer))
             (tasks
              (org-element-map tree 'inlinetask
                (lambda (task)
                  (list (org-element-property :todo-keyword task)
                        (org-element-property :raw-value task)
                        (org-element-property :tags task)
                        (org-element-property :level task)
                        (org-element-property :contents-begin task)))))
             (html (replace-regexp-in-string
                    "org[[:alnum:]]+"
                    "org-id"
                    (org-export-as 'html nil nil t nil))))
        (list tasks
              (not (null (string-match-p "Inline one" html)))
              (not (null (string-match-p "SCHEDULED" html)))
              (not (null (string-match-p "Effort" html)))
              html)))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_visibility_and_remove_end_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 4))
      (org-mode)
      (insert "* Parent\n")
      (insert "**** TODO Inline\n")
      (insert "Hidden body\n")
      (insert "**** END\n")
      (insert "** Child\nBody\n")
      (goto-char (point-min))
      (search-forward "Inline")
      (beginning-of-line)
      (let ((before (list (org-inlinetask-at-task-p)
                          (org-inlinetask-in-task-p)
                          (org-inlinetask-get-task-level))))
        (org-inlinetask-toggle-visibility 'fold)
        (let ((folded (org-fold-folded-p (line-end-position) 'headline)))
          (org-inlinetask-toggle-visibility 'unfold)
          (org-inlinetask-goto-end)
          (forward-line -1)
          (org-inlinetask-remove-END-maybe)
          (list before
                folded
                (org-fold-folded-p (line-end-position) 'headline)
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_fontify_edit_export_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (require 'ox-html)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 4)
          (org-inlinetask-show-first-star t)
          (org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE")))
          (org-export-with-toc nil))
      (org-mode)
      (insert "#+TITLE: Inline Font\n")
      (insert "* Parent\n")
      (insert "Before.\n")
      (insert "**** TODO Inline Alpha :old:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert "Body with *bold* and [[https://example.org][link]].\n")
      (insert "**** END\n")
      (insert "**** WAIT Inline Beta\n")
      (insert ":PROPERTIES:\n:Effort: 0:10\n:END:\n")
      (insert "Beta body.\n")
      (insert "**** END\n")
      (font-lock-ensure (point-min) (point-max))
      (let ((before
             (mapcar
              (lambda (needle)
                (save-excursion
                  (goto-char (point-min))
                  (search-forward needle)
                  (list needle
                        (org-inlinetask-in-task-p)
                        (get-text-property (match-beginning 0) 'face)
                        (get-text-property (match-beginning 0)
                                           'font-lock-fontified))))
              '("Inline Alpha" "Inline Beta" "bold" "link"))))
        (goto-char (point-min))
        (search-forward "Inline Alpha")
        (beginning-of-line)
        (org-inlinetask-toggle-visibility 'fold)
        (let ((folded (list (org-fold-folded-p (line-end-position)
                                               'headline)
                            (invisible-p
                             (save-excursion
                               (search-forward "Body with")
                               (point))))))
          (org-inlinetask-toggle-visibility 'unfold)
          (org-todo "DONE")
          (org-toggle-tag "old" 'off)
          (org-toggle-tag "new" 'on)
          (let* ((tree (org-element-parse-buffer))
                 (tasks
                  (org-element-map tree 'inlinetask
                    (lambda (task)
                      (list (org-element-property :todo-keyword task)
                            (org-element-property :raw-value task)
                            (org-element-property :tags task)
                            (org-element-property :scheduled task)))))
                 (html (replace-regexp-in-string
                        "org[[:alnum:]]+"
                        "org-id"
                        (org-export-as 'html nil nil t nil))))
            (list before
                  folded
                  tasks
                  (not (null (string-match-p "Inline Alpha" html)))
                  (not (null (string-match-p "DONE" html)))
                  (not (null (string-match-p "new" html)))
                  html
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_cycle_hook_odd_levels_error_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 3)
          (org-odd-levels-only t)
          (org-cycle-hook '(org-inlinetask-hide-tasks))
          (org-adapt-indentation t))
      (org-mode)
      (insert "* Parent\n")
      (insert "Intro\n")
      (insert "***** TODO Inline odd :tag:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert "Body one\n")
      (insert "***** END\n")
      (insert "** Child\n")
      (insert "Child body\n")
      (insert "***** TODO Inline no end\n")
      (insert "Single line\n")
      (font-lock-ensure (point-min) (point-max))
      (cl-labels
          ((inline-snapshot
            (label)
            (save-excursion
              (goto-char (point-min))
              (let (rows)
                (while (re-search-forward "^\\*\\{5,\\} " nil t)
                  (beginning-of-line)
                  (push (list label
                              (buffer-substring-no-properties
                               (line-beginning-position)
                               (line-end-position))
                              (org-inlinetask-at-task-p)
                              (org-inlinetask-in-task-p)
                              (org-inlinetask-get-task-level)
                              (org-fold-folded-p
                               (line-end-position) 'headline)
                              (get-text-property (point) 'face))
                        rows)
                  (forward-line 1))
                (nreverse rows)))))
        (let ((initial (inline-snapshot 'initial))
              contents children promote-error after-demote after-promote)
          (goto-char (point-min))
          (search-forward "Parent")
          (beginning-of-line)
          (org-cycle)
          (setq contents (inline-snapshot 'contents))
          (org-cycle)
          (setq children (inline-snapshot 'children))
          (goto-char (point-min))
          (search-forward "Inline odd")
          (beginning-of-line)
          (setq promote-error
                (condition-case err
                    (progn (org-inlinetask-promote) 'no-error)
                  (error (cons (car err) (cdr err)))))
          (org-inlinetask-demote)
          (setq after-demote
                (list (org-inlinetask-get-task-level)
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))))
          (org-inlinetask-promote)
          (setq after-promote
                (list (org-inlinetask-get-task-level)
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))))
          (list initial
                contents
                children
                promote-error
                after-demote
                after-promote
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_adjacent_boundary_cut_paste_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-inlinetask-min-level 4)
          (org-inlinetask-default-state "NEXT")
          (org-adapt-indentation t)
          (transient-mark-mode t))
      (org-mode)
      (insert "* Parent\n")
      (insert "Intro\n")
      (insert "**** TODO First inline\n")
      (insert "First body\n")
      (insert "**** END\n")
      (insert "**** WAIT Second inline\n")
      (insert "Second body\n")
      (insert "**** END\n")
      (insert "**** TODO Third no end\n")
      (insert "Third body\n")
      (insert "** After\n")
      (insert "After body\n")
      (font-lock-ensure (point-min) (point-max))
      (let (states)
        (cl-labels
            ((row
              (needle)
              (save-excursion
                (goto-char (point-min))
                (search-forward needle)
                (beginning-of-line)
                (list needle
                      (line-number-at-pos)
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))
                      (org-inlinetask-at-task-p)
                      (org-inlinetask-in-task-p)
                      (condition-case err
                          (org-inlinetask-get-task-level)
                        (error (cons (car err) (cdr err))))
                      (save-excursion
                        (condition-case err
                            (progn
                              (org-inlinetask-goto-beginning)
                              (line-number-at-pos))
                          (error (cons (car err) (cdr err)))))
                      (save-excursion
                        (condition-case err
                            (progn
                              (org-inlinetask-goto-end)
                              (line-number-at-pos))
                          (error (cons (car err) (cdr err)))))
                      (get-text-property (point) 'face))))
             (snapshot
              (label)
              (list label
                    (mapcar #'row
                            '("First inline" "First body"
                              "Second inline" "Second body"
                              "Third no end" "Third body" "After"))
                    (count-matches "^\\*\\{4,\\} " (point-min) (point-max))
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
          (push (snapshot 'initial) states)
          (goto-char (point-min))
          (search-forward "Second body")
          (beginning-of-line)
          (let ((nest-error
                 (condition-case err
                     (progn (org-inlinetask-insert-task nil) 'no-error)
                   (error (list (car err) (cadr err))))))
            (goto-char (point-min))
            (search-forward "Second inline")
            (beginning-of-line)
            (org-inlinetask-insert-task nil)
            (insert "Inserted before second\n")
            (push (snapshot 'after-boundary-insert) states)
            (goto-char (point-min))
            (search-forward "Third no end")
            (beginning-of-line)
            (org-inlinetask-goto-end)
            (insert "Line after third end calc\n")
            (push (snapshot 'after-no-end-goto-end) states)
            (goto-char (point-min))
            (search-forward "First inline")
            (beginning-of-line)
            (org-inlinetask-demote)
            (push (snapshot 'after-demote-first) states)
            (org-inlinetask-promote)
            (push (snapshot 'after-promote-first) states)
            (goto-char (point-min))
            (search-forward "First inline")
            (beginning-of-line)
            (let ((cut-start (line-number-at-pos)))
              (org-cut-subtree)
              (goto-char (point-min))
              (search-forward "After")
              (beginning-of-line)
              (org-paste-subtree 2)
              (push (snapshot 'after-cut-paste-first) states)
              (goto-char (point-min))
              (search-forward "After body")
              (end-of-line)
              (push-mark (line-beginning-position) nil t)
              (org-inlinetask-insert-task t)
              (push (snapshot 'after-region-no-state) states)
              (list nest-error
                    cut-start
                    (nreverse states)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_inlinetask_insert_promote_demote_export_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot promote an inline task at minimum level\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "* Regular heading\nBody.\n\n")
    (insert "*************** TODO Inline task A\n")
    (insert "Body of inline A.\n")
    (insert "*************** END\n\n")
    (insert "Another paragraph.\n\n")
    (insert "*************** DONE Inline task B\n")
    (insert "Body of inline B.\n")
    (insert "*************** END\n\n")
    (let ((snap (lambda (tag)
                  (list tag
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))
      (let ((initial (funcall snap 'initial)))
        ;; Promote inline A
        (goto-char (point-min))
        (search-forward "Inline task A")
        (beginning-of-line)
        (org-inlinetask-promote)
        (let ((after-promote (funcall snap 'promote)))
          ;; Demote back
          (goto-char (point-min))
          (search-forward "Inline task A")
          (beginning-of-line)
          (org-inlinetask-demote)
          (let ((after-demote (funcall snap 'demote)))
            ;; Export
            (let ((html (org-export-as 'html nil nil t '(:with-toc nil))))
              (list initial
                    after-promote
                    after-demote
                    html
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}
