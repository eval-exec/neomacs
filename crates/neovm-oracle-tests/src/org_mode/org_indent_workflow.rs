use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_indent_fold_edit_level_refresh_no_merge_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable prefix-info)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'org-indent)
  (require 'org-inlinetask)
  (with-temp-buffer
    (let ((org-indent-indentation-per-level 3)
          (org-adapt-indentation 'headline-data)
          (org-indent-mode-turns-off-org-adapt-indentation nil)
          (org-indent-mode-turns-on-hiding-stars t)
          (org-hide-leading-stars t)
          (org-cycle-global-at-bob t)
          (org-cycle-separator-lines 0)
          (org-inlinetask-min-level 5)
          (org-inlinetask-show-first-star t))
      (org-mode)
      (insert "#+STARTUP: content indent\n")
      (insert "* TODO Project :work:\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 1:00\n:END:\n")
      (insert "Project paragraph\n")
      (insert "- [ ] root item\n")
      (insert "  - [X] child item\n")
      (insert "    child continuation\n")
      (insert "** NEXT Alpha\nAlpha body\n")
      (insert "*** WAIT Alpha child\nAlpha child body\n")
      (insert "**** TODO Alpha fourth\nAlpha fourth body\n")
      (insert "***** Inline task\nInline task body\n***** END\n")
      (insert "** TODO Beta\nBeta body\n*** TODO Beta child\nBeta child body\n")
      (org-indent-mode 1)
      (org-indent-indent-buffer)
      (font-lock-ensure (point-min) (point-max))
      (let ((needles
             '("Project" "SCHEDULED:" ":Owner:" "Project paragraph"
               "root item" "child item" "Alpha" "Alpha child"
               "Alpha fourth" "Inline task" "Inline task body"
               "Beta" "Beta child"))
            states)
        (let ((prefix-info
               (lambda (pos)
                 (let ((lp (get-text-property pos 'line-prefix))
                       (wp (get-text-property pos 'wrap-prefix)))
                   (list
                    (and (stringp lp)
                         (list (length lp)
                               (substring-no-properties lp)
                               (get-text-property 0 'face lp)))
                    (and (stringp wp)
                         (list (length wp)
                               (substring-no-properties wp)
                               (get-text-property 0 'face wp)))))))
              (snapshot
               (lambda (label)
                 (font-lock-ensure (point-min) (point-max))
                 (list label
                       org-cycle-global-status
                       org-cycle-subtree-status
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (let ((pos (line-beginning-position)))
                              (list needle
                                    (line-number-at-pos pos)
                                    (org-current-level)
                                    (org-at-heading-p)
                                    (invisible-p pos)
                                    (get-text-property pos 'face)
                                    (funcall prefix-info pos)))))
                        needles)
                       (save-excursion
                         (goto-char (point-min))
                         (let (rows)
                           (while (not (eobp))
                             (let ((pos (line-beginning-position)))
                               (push
                                (list (buffer-substring-no-properties
                                       pos (line-end-position))
                                      (funcall prefix-info pos)
                                      (get-text-property pos 'invisible))
                                rows))
                             (forward-line 1))
                           (nreverse rows)))
                       (count-matches "^\\*+ " (point-min) (point-max))
                       (count-lines (point-min) (point-max))))))
          (push (funcall snapshot 'initial) states)
          (org-cycle-set-startup-visibility)
          (push (funcall snapshot 'startup) states)
          (goto-char (point-min))
          (search-forward "Alpha fourth")
          (beginning-of-line)
          (dotimes (_ 4)
            (org-cycle)
            (push (funcall snapshot 'cycle-alpha-fourth) states))
          (org-fold-hide-subtree)
          (org-end-of-subtree t t)
          (insert "**** TODO Inserted sibling\nInserted body\n")
          (push (funcall snapshot 'after-hidden-insert) states)
          (org-fold-show-all)
          (goto-char (point-min))
          (search-forward "Beta child")
          (beginning-of-line)
          (org-demote-subtree)
          (search-forward "Beta child")
          (beginning-of-line)
          (org-promote-subtree)
          (goto-char (point-min))
          (search-forward "root item")
          (end-of-line)
          (insert "\n  - [ ] inserted child\n    inserted continuation")
          (goto-char (point-min))
          (search-forward ":Effort:")
          (end-of-line)
          (insert "\n:Priority: A")
          (push (funcall snapshot 'after-edits) states)
          (org-indent-indent-buffer)
          (push (funcall snapshot 'after-reindent) states)
          (goto-char (point-min))
          (dotimes (_ 5)
            (org-cycle-global)
            (push (funcall snapshot 'global-cycle) states))
          (org-fold-show-all)
          (let* ((copied (filter-buffer-substring
                          (point-min) (point-max) nil))
                 (merged nil)
                 (prop-leak
                  (list (text-property-any 0 (length copied)
                                           'line-prefix nil copied)
                        (text-property-any 0 (length copied)
                                           'wrap-prefix nil copied))))
            (dolist (line (split-string
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           "\n" t))
              (when (string-match-p "^\\*+ .*\\*+ " line)
                (push line merged)))
            (list (nreverse states)
                  prop-leak
                  (nreverse merged)
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_indent_incremental_refresh_property_cleanup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((initial t nil headline-data t ((\"Root\" 1 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 6 1 nil ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 7 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 8 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 9 2 nil ((3 \"***\" org-indent) (6 \"***** \" org-indent) org-hide nil)) (\"Child body\" 10 2 nil ((6 \"     .\" org-indent) (6 \"     .\" org-indent) nil nil)) (\"Grandchild\" 11 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 12 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)))) (after-body-insert t nil headline-data t ((\"Root\" 1 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 6 1 nil ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 9 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 7 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 11 2 nil ((3 \"***\" org-indent) (6 \"***** \" org-indent) org-hide nil)) (\"Child body\" 12 2 nil ((6 \"     .\" org-indent) (6 \"     .\" org-indent) nil nil)) (\"Grandchild\" 13 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 14 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\"    deeper continuation\" ((2 \" .\" org-indent) (6 \" .    \" org-indent) nil nil)) (\"- [ ] item\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"  continuation\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"** NEXT Child\" ((3 \"***\" org-indent) (6 \"***** \" org-indent) org-hide nil)) (\"Child body\" ((6 \"     .\" org-indent) (6 \"     .\" org-indent) nil nil)) (\"*** TODO Grandchild\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)))) (after-demote-star-insert t nil headline-data t ((\"Root\" 1 1 t ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 t ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 t ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 6 1 t ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 9 1 t ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 7 1 t ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 11 3 t ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" 12 3 t ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"Grandchild\" 13 3 t ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 14 3 t ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\"*** NEXT Child\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"*** TODO Grandchild\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)))) (after-list-insert t nil headline-data t ((\"Root\" 1 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 6 1 nil ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 9 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 7 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 11 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" 12 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"Grandchild\" 13 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 14 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\"      nested continuation\" ((10 \"         .\" org-indent) (16 \"         .      \" org-indent) nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)))) (after-property-insert t nil headline-data t ((\"Root\" 1 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 7 1 nil ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 10 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 8 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 12 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" 13 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"Grandchild\" 14 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 15 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\":Effort: 0:45\" ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":END:\" ((0 \"\" nil) (2 \"* \" org-indent) org-drawer nil)) (\"Paragraph one\" ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"  paragraph continuation\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"    deeper continuation\" ((2 \" .\" org-indent) (6 \" .    \" org-indent) nil nil)) (\"- [ ] item\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"  continuation\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"*** NEXT Child\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"*** TODO Grandchild\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"    - [X] nested item\" ((10 \"         .\" org-indent) (16 \"         .      \" org-indent) nil nil)) (\"      nested continuation\" ((10 \"         .\" org-indent) (16 \"         .      \" org-indent) nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)))) (after-reenable t nil headline-data t ((\"Root\" 1 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-level-1 nil)) (\"SCHEDULED:\" 2 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":Owner:\" 4 1 nil ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\"Paragraph one\" 7 1 nil ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"item\" 10 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"continuation\" 8 1 nil ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"Child\" 12 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" 13 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"Grandchild\" 14 3 nil ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" 15 3 nil ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil))) ((\":Effort: 0:45\" ((0 \"\" nil) (2 \"* \" org-indent) org-special-keyword nil)) (\":END:\" ((0 \"\" nil) (2 \"* \" org-indent) org-drawer nil)) (\"Paragraph one\" ((2 \" .\" org-indent) (2 \" .\" org-indent) nil nil)) (\"  paragraph continuation\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"    deeper continuation\" ((2 \" .\" org-indent) (6 \" .    \" org-indent) nil nil)) (\"- [ ] item\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"  continuation\" ((2 \" .\" org-indent) (4 \" .  \" org-indent) nil nil)) (\"*** NEXT Child\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Child body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"*** TODO Grandchild\" ((6 \"******\" org-indent) (10 \"********* \" org-indent) org-hide nil)) (\"Grandchild body\" ((10 \"         .\" org-indent) (10 \"         .\" org-indent) nil nil)) (\"    - [X] nested item\" ((10 \"         .\" org-indent) (16 \"         .      \" org-indent) nil nil)) (\"      nested continuation\" ((10 \"         .\" org-indent) (16 \"         .      \" org-indent) nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil)) (\"\" (nil nil nil nil))))) (nil nil) (0 0) (1 1) \"* TODO Root\\nSCHEDULED: <2026-05-27 Wed>\\n:PROPERTIES:\\n:Owner: Ada\\n:Effort: 0:45\\n:END:\\nParagraph one\\n  paragraph continuation\\n    deeper continuation\\n- [ ] item\\n  continuation\\n*** NEXT Child\\nChild body\\n*** TODO Grandchild\\nGrandchild body\\n    - [X] nested item\\n      nested continuation\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-indent)
  (require 'org-list)
  (with-temp-buffer
    (let ((org-indent-indentation-per-level 4)
          (org-indent-boundary-char ?.)
          (org-adapt-indentation 'headline-data)
          (org-indent-mode-turns-off-org-adapt-indentation nil)
          (org-indent-mode-turns-on-hiding-stars t)
          (org-hide-leading-stars t)
          (org-inlinetask-min-level 7))
      (org-mode)
      (insert "* TODO Root\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "Paragraph one\n")
      (insert "- [ ] item\n  continuation\n")
      (insert "** NEXT Child\nChild body\n")
      (insert "*** TODO Grandchild\nGrandchild body\n")
      (org-indent-mode 1)
      (org-indent-indent-buffer)
      (font-lock-ensure (point-min) (point-max))
      (let (states)
        (cl-labels
            ((prefix-state
              (pos)
              (let ((lp (get-text-property pos 'line-prefix))
                    (wp (get-text-property pos 'wrap-prefix))
                    (face (get-text-property pos 'face))
                    (inv (get-text-property pos 'invisible)))
                (list
                 (and (stringp lp)
                      (list (length lp)
                            (substring-no-properties lp)
                            (get-text-property 0 'face lp)))
                 (and (stringp wp)
                      (list (length wp)
                            (substring-no-properties wp)
                            (get-text-property 0 'face wp)))
                 face
                 inv)))
             (find-line
              (needle)
              (save-excursion
                (goto-char (point-min))
                (search-forward needle)
                (line-beginning-position)))
             (snapshot
              (label)
              (font-lock-ensure (point-min) (point-max))
              (list
               label
               org-indent-mode
               org-indent-modified-headline-flag
               org-adapt-indentation
               org-hide-leading-stars
               (mapcar
                (lambda (needle)
                  (let ((pos (find-line needle)))
                    (list needle
                          (line-number-at-pos pos)
                          (save-excursion
                            (goto-char pos)
                            (org-current-level))
                          (org-at-heading-p)
                          (prefix-state pos))))
                '("Root" "SCHEDULED:" ":Owner:" "Paragraph one"
                  "item" "continuation" "Child" "Child body"
                  "Grandchild" "Grandchild body"))
               (mapcar
                (lambda (line)
                  (let ((pos (line-beginning-position line)))
                    (list (buffer-substring-no-properties
                           pos (line-end-position line))
                          (prefix-state pos))))
                (number-sequence 1 (count-lines (point-min) (point-max)))))))
          (push (snapshot 'initial) states)
          (goto-char (find-line "Paragraph one"))
          (end-of-line)
          (insert "\n  paragraph continuation\n    deeper continuation")
          (push (snapshot 'after-body-insert) states)
          (goto-char (find-line "Child"))
          (forward-char 1)
          (insert "*")
          (push (snapshot 'after-demote-star-insert) states)
          (goto-char (find-line "Grandchild body"))
          (end-of-line)
          (insert "\n    - [X] nested item\n      nested continuation")
          (push (snapshot 'after-list-insert) states)
          (goto-char (find-line ":Owner:"))
          (end-of-line)
          (insert "\n:Effort: 0:45")
          (push (snapshot 'after-property-insert) states)
          (let* ((before-disable
                  (list (text-property-any (point-min) (point-max)
                                           'line-prefix nil)
                        (text-property-any (point-min) (point-max)
                                           'wrap-prefix nil)))
                 (copied-before
                  (filter-buffer-substring (point-min) (point-max) nil))
                 (copy-before-props
                  (list (text-property-any 0 (length copied-before)
                                           'line-prefix nil copied-before)
                        (text-property-any 0 (length copied-before)
                                           'wrap-prefix nil copied-before))))
            (org-indent-mode -1)
            (let ((after-disable
                   (list (text-property-any (point-min) (point-max)
                                            'line-prefix nil)
                         (text-property-any (point-min) (point-max)
                                            'wrap-prefix nil))))
              (org-indent-mode 1)
              (org-indent-indent-buffer)
              (push (snapshot 'after-reenable) states)
               (list (nreverse states)
                     before-disable
                     copy-before-props
                     after-disable
                     (buffer-substring-no-properties
                      (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_indent_deep_heading_cycle_visibility_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((#(\"L1\" 0 2 (face org-level-1 wrap-prefix #(\"* \" 0 2 (face org-indent)) line-prefix \"\")) 1 1 \"\" #(\"* \" 0 2 (face org-indent)) org-level-1 nil) (#(\"L2\" 0 2 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil) (#(\"L3\" 0 2 (face org-level-3 wrap-prefix #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) line-prefix #(\"**\" 0 2 (face org-indent)))) 3 3 #(\"**\" 0 2 (face org-indent)) #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) org-hide nil) (#(\"L4\" 0 2 (face org-level-4 wrap-prefix #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) line-prefix #(\"***\" 0 3 (face org-indent)))) 4 4 #(\"***\" 0 3 (face org-indent)) #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) org-hide nil) (#(\"L5\" 0 2 (face org-level-5 wrap-prefix #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) line-prefix #(\"****\" 0 4 (face org-indent)))) 5 5 #(\"****\" 0 4 (face org-indent)) #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) org-hide nil) (#(\"L2b\" 0 3 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil)) ((#(\"L1\" 0 2 (face org-level-1 wrap-prefix #(\"* \" 0 2 (face org-indent)) line-prefix \"\")) 1 1 \"\" #(\"* \" 0 2 (face org-indent)) org-level-1 nil) (#(\"L2\" 0 2 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil) (#(\"L3\" 0 2 (face org-level-3 wrap-prefix #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) line-prefix #(\"**\" 0 2 (face org-indent)))) 3 3 #(\"**\" 0 2 (face org-indent)) #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) org-hide nil) (#(\"L4\" 0 2 (face org-level-4 wrap-prefix #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) line-prefix #(\"***\" 0 3 (face org-indent)))) 4 4 #(\"***\" 0 3 (face org-indent)) #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) org-hide nil) (#(\"L5\" 0 2 (face org-level-5 wrap-prefix #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) line-prefix #(\"****\" 0 4 (face org-indent)))) 5 5 #(\"****\" 0 4 (face org-indent)) #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) org-hide nil) (#(\"L2b\" 0 3 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil)) ((#(\"L1\" 0 2 (face org-level-1 wrap-prefix #(\"* \" 0 2 (face org-indent)) line-prefix \"\")) 1 1 \"\" #(\"* \" 0 2 (face org-indent)) org-level-1 nil) (#(\"L2\" 0 2 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil) (#(\"L3\" 0 2 (face org-level-3 wrap-prefix #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) line-prefix #(\"**\" 0 2 (face org-indent)))) 3 3 #(\"**\" 0 2 (face org-indent)) #(\"***** \" 0 2 (face org-indent) 2 6 (face org-indent)) org-hide nil) (#(\"L4\" 0 2 (face org-level-4 wrap-prefix #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) line-prefix #(\"***\" 0 3 (face org-indent)))) 4 4 #(\"***\" 0 3 (face org-indent)) #(\"******* \" 0 3 (face org-indent) 3 8 (face org-indent)) org-hide nil) (#(\"L5\" 0 2 (face org-level-5 wrap-prefix #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) line-prefix #(\"****\" 0 4 (face org-indent)))) 5 5 #(\"****\" 0 4 (face org-indent)) #(\"********* \" 0 4 (face org-indent) 4 10 (face org-indent)) org-hide nil) (#(\"L2b\" 0 3 (face org-level-2 wrap-prefix #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) line-prefix #(\"*\" 0 1 (face org-indent)))) 2 2 #(\"*\" 0 1 (face org-indent)) #(\"*** \" 0 1 (face org-indent) 1 4 (face org-indent)) org-hide nil)) \"* L1\\nbody 1\\n** L2\\nbody 2\\n*** L3\\nbody 3\\n**** L4\\nbody 4\\n***** L5\\nbody 5\\n** L2b\\nbody 2b\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-indent)
  (require 'org-cycle)
  (with-temp-buffer
    (let ((org-startup-indented t)
          (org-hide-leading-stars t))
      (org-mode)
      (org-indent-mode 1)
      (insert "* L1\nbody 1\n")
      (insert "** L2\nbody 2\n")
      (insert "*** L3\nbody 3\n")
      (insert "**** L4\nbody 4\n")
      (insert "***** L5\nbody 5\n")
      (insert "** L2b\nbody 2b\n")
      ;; Per-heading indent state
      (let ((heading-state
             (lambda ()
               (font-lock-ensure (point-min) (point-max))
               (let (out)
                 (goto-char (point-min))
                 (while (re-search-forward "^\\(\\*+\\) \\(.*\\)$" nil t)
                   (let ((beg (line-beginning-position)))
                     (push (list (match-string 2)
                                 (length (match-string 1))
                                 (org-outline-level)
                                 (get-text-property beg 'line-prefix)
                                 (get-text-property beg 'wrap-prefix)
                                 (get-text-property beg 'face)
                                 (invisible-p beg))
                           out)))
                 (nreverse out)))))
        ;; Global cycle
        (dotimes (_ 3) (org-cycle-global))
        (let ((after-cycle (funcall heading-state)))
          ;; Show all
          (org-fold-show-all)
          (let ((after-show (funcall heading-state)))
            ;; Indent buffer
            (org-indent-indent-buffer)
            (let ((after-indent (funcall heading-state)))
              (list after-cycle
                    after-show
                    after-indent
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}
