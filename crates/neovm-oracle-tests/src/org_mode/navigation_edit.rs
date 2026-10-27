use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_mark_narrow_unindent_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* A\\n  Body A\\n  ** B\\n    Body B\\n    - item\\n* C\\n  Body C\\n\" \"  ** B\\n\" \"  ** B\\n\" (38 43) (38 43) \"item\\n\" \"* A\\n  Body A\\n  ** B\\n    Body B\\n    - item\\n* C\\n  Body C\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "  * A\n")
    (insert "    Body A\n")
    (insert "    ** B\n")
    (insert "      Body B\n")
    (insert "      - item\n")
    (insert "  * C\n")
    (insert "    Body C\n")
    (org-unindent-buffer)
    (let ((after-unindent
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "** B")
      (beginning-of-line)
      (let ((sibling-prev (save-excursion
                            (condition-case err
                                (progn (org-goto-sibling 'previous)
                                       (thing-at-point 'line t))
                              (error (cons (car err) (cdr err))))))
            (first-child (save-excursion
                           (org-goto-first-child)
                           (thing-at-point 'line t))))
        (search-forward "item")
        (org-mark-element)
        (let ((mark-span (list (point) (mark))))
          (org-narrow-to-element)
          (let ((narrow-text
                 (buffer-substring-no-properties (point-min) (point-max)))
                (narrow-limits (list (point-min) (point-max))))
            (widen)
            (list after-unindent
                  sibling-prev
                  first-child
                  mark-span
                  narrow-limits
                  narrow-text
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_copy_visible_clone_subtree_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Project\\n* Tail\\n\" \"* Project\\n** TODO Task\\nSCHEDULED: <2026-05-27 Wed>\\nBody task\\n*** Child\\nChild body\\n** Keep\\nKeep body\\n* Tail\\nTail body\\n** TODO Task\\nSCHEDULED: <2026-05-27 Wed>\\nBody task\\n*** Child\\nChild body\\n\" ((1 \"Project\" nil) (2 \"Task\" \"<2026-05-27 Wed>\") (3 \"Child\" nil) (2 \"Task\" \"<2026-06-03 Wed>\") (3 \"Child\" nil) (2 \"Task\" \"<2026-06-10 Wed>\") (3 \"Child\" nil) (2 \"Keep\" nil) (1 \"Tail\" nil) (2 \"Task\" \"<2026-05-27 Wed>\") (3 \"Child\" nil)) 2 \"Task\" \"* Project\\n** TODO Task\\nSCHEDULED: <2026-05-27 Wed>\\nBody task\\n*** Child\\nChild body\\n** TODO Task\\nSCHEDULED: <2026-06-03 Wed>\\nBody task\\n*** Child\\nChild body\\n** TODO Task\\nSCHEDULED: <2026-06-10 Wed>\\nBody task\\n*** Child\\nChild body\\n** Keep\\nKeep body\\n* Tail\\nTail body\\n** TODO Task\\nSCHEDULED: <2026-05-27 Wed>\\nBody task\\n*** Child\\nChild body\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((kill-ring nil)
          (kill-ring-yank-pointer nil)
          (org-yank-folded-subtrees nil))
      (org-mode)
      (insert "* Project\n")
      (insert "** TODO Task\n")
      (insert "SCHEDULED: <2026-05-27 Wed>\n")
      (insert "Body task\n")
      (insert "*** Child\nChild body\n")
      (insert "** Keep\nKeep body\n")
      (insert "* Tail\nTail body\n")
      (goto-char (point-min))
      (org-fold-hide-sublevels 1)
      (org-copy-visible (point-min) (point-max))
      (let ((visible-copy (current-kill 0 t)))
        (org-fold-show-all)
        (goto-char (point-min))
        (search-forward "Task")
        (beginning-of-line)
        (org-copy-subtree 1)
        (goto-char (point-max))
        (org-paste-subtree 2)
        (let ((after-paste
               (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "Task")
          (beginning-of-line)
          (org-clone-subtree-with-time-shift 2 "+1w")
          (let ((nav nil))
            (goto-char (point-min))
            (while (re-search-forward "^\\*+ " nil t)
              (push (list (org-outline-level)
                          (org-get-heading t t t t)
                          (org-entry-get nil "SCHEDULED"))
                    nav))
            (goto-char (point-min))
            (search-forward "Child")
            (beginning-of-line)
            (list visible-copy
                  after-paste
                  (nreverse nav)
                  (org-up-heading-safe)
                  (org-get-heading t t t t)
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_navigation_hidden_narrow_deep_faces_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable states)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-cycle-level-faces t)
          (org-level-color-stars-only nil))
      (org-mode)
      (insert "* Project\n")
      (insert "Intro paragraph.\n")
      (insert "** Alpha\n")
      (insert "Alpha body\n")
      (insert "*** Alpha child\n")
      (insert "Alpha child body\n")
      (insert "** COMMENT Folded comment\n")
      (insert "Comment body\n")
      (insert "*** Hidden comment child\n")
      (insert "Hidden body\n")
      (insert "** Beta archived :ARCHIVE:\n")
      (insert "Beta body\n")
      (insert "*** Beta child\n")
      (insert "Beta child body\n")
      (insert "** Gamma\n")
      (insert ":PROPERTIES:\n:Owner: me\n:END:\n")
      (insert "Gamma body\n")
      (insert "*** Gamma child\n")
      (insert "Gamma child body\n")
      (insert "**** Deep L4\n")
      (insert "Deep body\n")
      (insert "***** Deep L5\n")
      (insert "Deeper body\n")
      (insert "* Tail\nTail body\n")
      (font-lock-ensure (point-min) (point-max))
      (let ((heading-state
             (lambda (label)
               (list label
                     (- (point) (point-min))
                     (line-number-at-pos)
                     (org-outline-level)
                     (org-get-heading t t t t)
                     (invisible-p (line-beginning-position))
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))))
            (goto-heading
             (lambda (needle)
               (goto-char (point-min))
               (search-forward needle)
               (beginning-of-line)))
            (hidden-state
             (lambda (label needles)
               (cons label
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle
                                (- (point) (point-min))
                                (invisible-p (point)))))
                      needles)))))
            states)
        (funcall goto-heading "* Project")
        (org-fold-hide-subtree)
        (push (funcall hidden-state
                       'project-hidden
                       '("Intro" "Alpha body" "Folded comment"
                         "Beta body" "Gamma" "Tail"))
              states)
        (org-fold-show-all)
        (funcall goto-heading "** Alpha")
        (org-fold-hide-subtree)
        (push (list 'alpha-boundaries
                    (save-excursion
                      (funcall goto-heading "** Alpha")
                      (list (- (org-end-of-subtree nil nil) (point-min))
                            (line-number-at-pos)))
                    (save-excursion
                      (funcall goto-heading "** Alpha")
                      (list (- (org-end-of-subtree t nil) (point-min))
                            (line-number-at-pos)))
                    (save-excursion
                      (funcall goto-heading "** Alpha")
                      (list (- (org-end-of-subtree t t) (point-min))
                            (line-number-at-pos))))
              states)
        (push (funcall hidden-state
                       'alpha-hidden
                       '("Alpha body" "Alpha child" "Folded comment"
                         "Beta archived" "Gamma"))
              states)
        (funcall goto-heading "** Alpha")
        (org-forward-heading-same-level 1 nil)
        (push (funcall heading-state 'same-level-visible-1) states)
        (funcall goto-heading "** Alpha")
        (org-forward-heading-same-level 2 t)
        (push (funcall heading-state 'same-level-invisible-2) states)
        (org-fold-hide-sublevels 2)
        (funcall goto-heading "* Project")
        (org-next-visible-heading 1)
        (push (funcall heading-state 'next-visible-after-project) states)
        (org-next-visible-heading 1)
        (push (funcall heading-state 'next-visible-second) states)
        (org-previous-visible-heading 1)
        (push (funcall heading-state 'previous-visible-back) states)
        (org-fold-show-all)
        (funcall goto-heading "*** Gamma child")
        (org-narrow-to-subtree)
        (let ((narrow-limits (list (- (point-min) 1) (- (point-max) 1))))
          (goto-char (point-min))
          (search-forward "Deep L5")
          (beginning-of-line)
          (let ((up1 (progn
                       (org-up-heading-safe)
                       (funcall heading-state 'up-from-l5)))
                (up2 (progn
                       (org-up-heading-safe)
                       (funcall heading-state 'up-from-l4)))
                (up3 (progn
                       (org-up-heading-safe)
                       (funcall heading-state 'up-from-child))))
            (push (list 'narrowed-up narrow-limits up1 up2 up3)
                  states)))
        (widen)
        (font-lock-ensure (point-min) (point-max))
        (push (let (faces)
                (dolist (needle '("Gamma child" "Deep L4" "Deep L5"))
                  (goto-char (point-min))
                  (search-forward needle)
                  (push (list needle
                              (get-text-property
                               (line-beginning-position) 'face)
                              (get-text-property
                               (match-beginning 0) 'face)
                              (get-text-property
                               (match-beginning 0) 'font-lock-fontified))
                        faces))
                (cons 'deep-heading-faces (nreverse faces)))
              states)
        (push (buffer-substring-no-properties (point-min) (point-max))
              states)
        (nreverse states))))"##,
        expect,
    );
}

#[test]
fn org_outline_path_entry_position_level_visibility_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 1 \"Project\" (\"work\") \"Ada\" nil t) (6 2 \"Design\" (\"deep\") \"Ada\" nil t) (8 3 \"Frontend\" nil \"Ada\" nil t) (12 5 \"Sub component\" nil \"Ada\" nil t) (10 4 \"WAIT Component A\" nil \"Ada\" nil t) (8 3 \"Frontend\" nil \"Ada\" nil t) (14 3 \"Backend\" nil \"Ada\" nil t) 15 \"* TODO Project :work:\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\nProject body.\\n** DONE Design :deep:\\nDesign body.\\n*** TODO Frontend\\nFrontend body.\\n**** WAIT Component A\\nCompA body.\\n***** DONE Sub component\\nSub body.\\n*** TODO Backend\\nBackend body.\\n** NEXT Testing\\nTesting body.\\n* Archive :archive:\\nArchive body.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Project :work:\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
    (insert "Project body.\n")
    (insert "** DONE Design :deep:\n")
    (insert "Design body.\n")
    (insert "*** TODO Frontend\n")
    (insert "Frontend body.\n")
    (insert "**** WAIT Component A\n")
    (insert "CompA body.\n")
    (insert "***** DONE Sub component\n")
    (insert "Sub body.\n")
    (insert "*** TODO Backend\n")
    (insert "Backend body.\n")
    (insert "** NEXT Testing\n")
    (insert "Testing body.\n")
    (insert "* Archive :archive:\n")
    (insert "Archive body.\n")
    ;; Track position, level, heading, outline path at each step
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (org-outline-level)
                         (org-get-heading t t t t)
                         (org-get-tags nil t)
                         (org-entry-get nil "Owner" t)
                         (invisible-p (point))
                         (org-at-heading-p)))))
      (goto-char (point-min))
      (let ((at-root (funcall track)))
        (org-next-visible-heading 1)
        (let ((at-design (funcall track)))
          (org-next-visible-heading 1)
          (let ((at-frontend (funcall track)))
            ;; Go to deeply nested
            (goto-char (point-min))
            (search-forward "Sub component")
            (beginning-of-line)
            (let ((at-sub (funcall track)))
              ;; Up heading
              (org-up-heading-safe)
              (let ((up-to-4 (funcall track)))
                (org-up-heading-safe)
                (let ((up-to-3 (funcall track)))
                  ;; Forward same level
                  (org-forward-heading-same-level 1)
                  (let ((same-level (funcall track)))
                    ;; End of subtree
                    (goto-char (point-min))
                    (search-forward "Design")
                    (beginning-of-line)
                    (let ((end-pos (progn (org-end-of-subtree) (point))))
                      (list at-root
                            at-design
                            at-frontend
                            at-sub
                            up-to-4
                            up-to-3
                            same-level
                            (line-number-at-pos end-pos)
                            (buffer-substring-no-properties
                             (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_up_forward_end_subtree_cycle_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 \"**** Sub 1\" 4 nil) (3 \"*** Component X\" 3 nil) (2 \"** Module A\" 2 nil) (10 \"** Module B\" 2 nil) 9 (6 \"**** Sub 2\" 4 nil) \"* Project\\n** Module A\\n*** Component X\\n**** Sub 1\\nBody sub 1.\\n**** Sub 2\\nBody sub 2.\\n*** Component Y\\nBody Y.\\n** Module B\\n*** Component Z\\nBody Z.\\n\\n** Module C\\n*** Component W\\nBody W.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Project\n")
    (insert "** Module A\n")
    (insert "*** Component X\n")
    (insert "**** Sub 1\nBody sub 1.\n")
    (insert "**** Sub 2\nBody sub 2.\n")
    (insert "*** Component Y\nBody Y.\n")
    (insert "** Module B\n")
    (insert "*** Component Z\nBody Z.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At Sub 1
      (goto-char (point-min))
      (search-forward "Sub 1")
      (beginning-of-line)
      (let ((at-sub1 (funcall track)))
        ;; Up to Component X
        (org-up-heading-safe)
        (let ((up-to-comp-x (funcall track)))
          ;; Up to Module A
          (org-up-heading-safe)
          (let ((up-to-mod-a (funcall track)))
            ;; Forward same level to Module B
            (org-forward-heading-same-level 1)
            (let ((fwd-to-mod-b (funcall track)))
              ;; End of subtree of Module A
              (goto-char (point-min))
              (search-forward "Module A")
              (beginning-of-line)
              (let ((end-of-mod-a (progn (org-end-of-subtree) (point))))
                ;; Edit: insert new heading under Module B
                (goto-char (point-max))
                (insert "** Module C\n*** Component W\nBody W.\n")
                ;; Re-track
                (goto-char (point-min))
                (search-forward "Sub 2")
                (beginning-of-line)
                (let ((at-sub2 (funcall track)))
                  (list at-sub1
                        up-to-comp-x
                        up-to-mod-a
                        fwd-to-mod-b
                        (line-number-at-pos end-of-mod-a)
                        at-sub2
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_sixteen_heading_tree_fold_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"*** S2C1\\nBody.\\n\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* S1\n")
    (insert "** S1A\n")
    (insert "*** S1A1\nBody.\n\n")
    (insert "*** S1A2\nBody.\n\n")
    (insert "** S1B\n")
    (insert "*** S1B1\nBody.\n\n")
    (insert "*** S1B2\nBody.\n\n")
    (insert "* S2\n")
    (insert "** S2A\n")
    (insert "*** S2A1\nBody.\n\n")
    (insert "** S2B\n")
    (insert "*** S2B1\nBody.\n\n")
    (insert "** S2C\n")
    (insert("*** S2C1\nBody.\n\n")
    (insert "** S2D\n")
    (insert("*** S2D1\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At S1A1
      (goto-char (point-min))
      (search-forward "S1A1")
      (beginning-of-line)
      (let ((at-s1a1 (funcall track)))
        ;; Forward to S1A2
        (org-forward-heading-same-level 1)
        (let ((at-s1a2 (funcall track)))
          ;; Up to S1A
          (org-up-heading-safe)
          (let ((at-s1a (funcall track)))
            ;; Forward to S1B
            (org-forward-heading-same-level 1)
            (let ((at-s1b (funcall track)))
              ;; Down to S1B1
              (org-next-visible-heading 1)
              (let ((at-s1b1 (funcall track)))
                ;; Forward to S1B2
                (org-forward-heading-same-level 1)
                (let ((at-s1b2 (funcall track)))
                  ;; Jump to S2D1
                  (goto-char (point-min))
                  (search-forward "S2D1")
                  (beginning-of-line)
                  (let ((at-s2d1 (funcall track)))
                    ;; Edit: insert S1B3 under S1B
                    (goto-char (point-min))
                    (search-forward "S1B2")
                    (end-of-line)
                    (insert "\n*** S1B3\nBody.\n")
                    ;; Fold S1 subtree
                    (goto-char (point-min))
                    (search-forward "S1")
                    (beginning-of-line)
                    (org-fold-subtree)
                    (let ((after-fold (funcall track)))
                      (list at-s1a1 at-s1a2 at-s1a at-s1b at-s1b1 at-s1b2 at-s2d1
                            after-fold
                            (buffer-substring-no-properties
                             (point-min) (point-max))))))))))))))))"##,
        expect,
    );
}
