use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_element_navigation_positions_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"Paragraph\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "Paragraph one.\n\n")
    (insert "- item one\n- item two\n\n")
    (insert "#+begin_quote\nquoted\n#+end_quote\n")
    (insert "** B\nBody B\n")
    (insert "* C\nBody C\n")
    (let ((snap (lambda (label)
                  (let ((e (org-element-at-point)))
                    (list label
                          (point)
                          (org-element-type e)
                          (org-element-property :begin e)
                          (org-element-property :end e)
                          (thing-at-point 'line t)))))
          states)
      (goto-char (point-min))
      (push (funcall snap 'start) states)
      (org-forward-element)
      (push (funcall snap 'forward-headline) states)
      (search-forward "Paragraph")
      (push (funcall snap 'paragraph) states)
      (org-forward-element)
      (push (funcall snap 'forward-list) states)
      (org-down-element)
      (push (funcall snap 'down-item) states)
      (org-up-element)
      (push (funcall snap 'up-list) states)
      (org-forward-element)
      (push (funcall snap 'forward-quote) states)
      (org-backward-element)
      (push (funcall snap 'backward-list) states)
      (goto-char (point-min))
      (search-forward "** B")
      (beginning-of-line)
      (push (list 'end-subtree
                  (org-end-of-subtree t nil)
                  (save-excursion
                    (org-end-of-subtree t t)
                    (point))
                  (line-number-at-pos)))
            states)
      (nreverse states))))"##,
        expect,
    );
}

#[test]
fn org_drag_transpose_element_buffer_integrity_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* H\n")
    (insert "First paragraph.\n\n")
    (insert "#+begin_quote\nQuote block.\n#+end_quote\n\n")
    (insert "- item one\n- item two\n\n")
    (insert "Final paragraph.\n")
    (goto-char (point-min))
    (search-forward "begin_quote")
    (beginning-of-line)
    (org-drag-element-forward)
    (let ((after-forward
           (buffer-substring-no-properties (point-min) (point-max))))
      (search-forward "Final paragraph")
      (beginning-of-line)
      (org-drag-element-backward)
      (let ((after-backward
             (buffer-substring-no-properties (point-min) (point-max))))
        (search-forward "item two")
        (beginning-of-line)
        (org-transpose-element)
        (list after-forward
              after-backward
              (buffer-substring-no-properties (point-min) (point-max))
              (org-element-map (org-element-parse-buffer)
                  '(paragraph quote-block plain-list item)
                (lambda (e)
                  (list (org-element-type e)
                        (org-element-property :begin e)
                        (org-element-property :end e))))))))"##,
        expect,
    );
}

#[test]
fn org_mark_narrow_unindent_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
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
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_copy_visible_clone_subtree_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
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
                   (point-min) (point-max))))))))"##,
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

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 53 60)""#]];
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
                         (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_up_down_forward_backward_edit_fold_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\nBody alpha.\n\n")
    (insert "** Beta\nBody beta.\n\n")
    (insert "*** Gamma\nBody gamma.\n\n")
    (insert "** Delta\nBody delta.\n\n")
    (insert "* Epsilon\nBody epsilon.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At Gamma
      (goto-char (point-min))
      (search-forward "Gamma")
      (beginning-of-line)
      (let ((at-gamma (funcall track)))
        ;; Up to Beta
        (org-up-heading-safe)
        (let ((up-to-beta (funcall track)))
          ;; Forward to Delta
          (org-forward-heading-same-level 1)
          (let ((fwd-to-delta (funcall track)))
            ;; Backward to Beta
            (org-backward-heading-same-level 1)
            (let ((back-to-beta (funcall track)))
              ;; Down to Gamma
              (org-next-visible-heading 1)
              (let ((down-to-gamma (funcall track)))
                ;; Edit: insert under Delta
                (goto-char (point-min))
                (search-forward "Delta")
                (end-of-line)
                (insert "\n*** Zeta\nBody zeta.\n")
                ;; Fold Beta subtree
                (goto-char (point-min))
                (search-forward "Beta")
                (beginning-of-line)
                (org-fold-subtree)
                (let ((after-fold (funcall track)))
                  (list at-gamma
                        up-to-beta
                        fwd-to-delta
                        back-to-beta
                        down-to-gamma
                        after-fold
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_level_jump_fold_edit_reparse_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Project\n")
    (insert "** Module A\n")
    (insert "*** Component X\nBody X.\n\n")
    (insert "*** Component Y\nBody Y.\n\n")
    (insert "** Module B\n")
    (insert "*** Component Z\nBody Z.\n\n")
    (insert "* Archive\n")
    (insert "** Old stuff\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At Component X
      (goto-char (point-min))
      (search-forward "Component X")
      (beginning-of-line)
      (let ((at-x (funcall track)))
        ;; Up to Module A
        (org-up-heading-safe)
        (let ((up-to-mod-a (funcall track)))
          ;; Forward to Module B
          (org-forward-heading-same-level 1)
          (let ((fwd-to-mod-b (funcall track)))
            ;; Down to Component Z
            (org-next-visible-heading 1)
            (let ((at-z (funcall track)))
              ;; Forward to Archive (cross parent)
              (goto-char (point-min))
              (search-forward "Archive")
              (beginning-of-line)
              (let ((at-archive (funcall track)))
                ;; Edit: insert under Module B
                (goto-char (point-min))
                (search-forward "Module B")
                (end-of-line)
                (insert "\n*** Component W\nBody W.\n")
                ;; Fold Module A
                (goto-char (point-min))
                (search-forward "Module A")
                (beginning-of-line)
                (org-fold-subtree)
                (let ((after-fold (funcall track)))
                  (list at-x up-to-mod-a fwd-to-mod-b at-z at-archive
                        after-fold
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_siblings_descendants_edit_fold_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "** Beta\n")
    (insert "*** Gamma\nBody.\n\n")
    (insert "*** Delta\nBody.\n\n")
    (insert "** Epsilon\n")
    (insert "*** Zeta\nBody.\n\n")
    (insert "* Eta\n")
    (insert "** Theta\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At Gamma
      (goto-char (point-min))
      (search-forward "Gamma")
      (beginning-of-line)
      (let ((at-gamma (funcall track)))
        ;; Next sibling: Delta
        (org-forward-heading-same-level 1)
        (let ((at-delta (funcall track)))
          ;; Up to Beta
          (org-up-heading-safe)
          (let ((at-beta (funcall track)))
            ;; Forward to Epsilon
            (org-forward-heading-same-level 1)
            (let ((at-epsilon (funcall track)))
              ;; Down to Zeta
              (org-next-visible-heading 1)
              (let ((at-zeta (funcall track)))
                ;; Jump to Eta
                (goto-char (point-min))
                (search-forward "Eta")
                (beginning-of-line)
                (let ((at-eta (funcall track)))
                  ;; Edit: insert under Epsilon
                  (goto-char (point-min))
                  (search-forward "Epsilon")
                  (end-of-line)
                  (insert "\n*** Iota\nBody.\n")
                  ;; Fold Alpha subtree
                  (goto-char (point-min))
                  (search-forward "Alpha")
                  (beginning-of-line)
                  (org-fold-subtree)
                  (let ((after-fold (funcall track)))
                    (list at-gamma at-delta at-beta at-epsilon at-zeta
                          at-eta after-fold
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_multi_level_cycle_edit_fold_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"^\\\\* A$\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* A\n")
    (insert "** A1\n")
    (insert "*** A1a\nBody.\n\n")
    (insert "*** A1b\nBody.\n\n")
    (insert "** A2\n")
    (insert "*** A2a\nBody.\n\n")
    (insert "* B\n")
    (insert "** B1\n")
    (insert "*** B1a\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At A1a
      (goto-char (point-min))
      (search-forward "A1a")
      (beginning-of-line)
      (let ((at-a1a (funcall track)))
        ;; Next sibling: A1b
        (org-forward-heading-same-level 1)
        (let ((at-a1b (funcall track)))
          ;; Up to A1
          (org-up-heading-safe)
          (let ((at-a1 (funcall track)))
            ;; Forward to A2
            (org-forward-heading-same-level 1)
            (let ((at-a2 (funcall track)))
              ;; Down to A2a
              (org-next-visible-heading 1)
              (let ((at-a2a (funcall track)))
                ;; Edit: insert under A2
                (goto-char (point-min))
                (search-forward "A2")
                (end-of-line)
                (insert "\n*** A2b\nBody.\n")
                ;; Fold A subtree
                (goto-char (point-min))
                (search-forward "^\\* A$")
                (beginning-of-line)
                (org-fold-subtree)
                (let ((after-fold (funcall track)))
                  (list at-a1a at-a1b at-a1 at-a2 at-a2a
                        after-fold
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_complex_tree_fold_edit_show_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* R\n")
    (insert "** R1\n")
    (insert "*** R1a\nBody.\n\n")
    (insert "*** R1b\nBody.\n\n")
    (insert "** R2\n")
    (insert "*** R2a\nBody.\n\n")
    (insert "*** R2b\nBody.\n\n")
    (insert "** R3\n")
    (insert "*** R3a\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At R1a
      (goto-char (point-min))
      (search-forward "R1a")
      (beginning-of-line)
      (let ((at-r1a (funcall track)))
        ;; Forward to R1b
        (org-forward-heading-same-level 1)
        (let ((at-r1b (funcall track)))
          ;; Up to R1
          (org-up-heading-safe)
          (let ((at-r1 (funcall track)))
            ;; Forward to R2
            (org-forward-heading-same-level 1)
            (let ((at-r2 (funcall track)))
              ;; Forward to R3
              (org-forward-heading-same-level 1)
              (let ((at-r3 (funcall track)))
                ;; Down to R3a
                (org-next-visible-heading 1)
                (let ((at-r3a (funcall track)))
                  ;; Edit: insert R2c under R2
                  (goto-char (point-min))
                  (search-forward "R2b")
                  (end-of-line)
                  (insert "\n*** R2c\nBody.\n")
                  ;; Fold R1 subtree
                  (goto-char (point-min))
                  (search-forward "R1")
                  (beginning-of-line)
                  (org-fold-subtree)
                  (let ((after-fold (funcall track)))
                    (list at-r1a at-r1b at-r1 at-r2 at-r3 at-r3a
                          after-fold
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_ten_heading_tree_fold_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* T1\n")
    (insert "** T1a\n")
    (insert "*** T1a1\nBody.\n\n")
    (insert "*** T1a2\nBody.\n\n")
    (insert "** T1b\n")
    (insert "*** T1b1\nBody.\n\n")
    (insert "* T2\n")
    (insert "** T2a\n")
    (insert "*** T2a1\nBody.\n\n")
    (insert "** T2b\n")
    (insert "*** T2b1\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At T1a1
      (goto-char (point-min))
      (search-forward "T1a1")
      (beginning-of-line)
      (let ((at-t1a1 (funcall track)))
        ;; Forward to T1a2
        (org-forward-heading-same-level 1)
        (let ((at-t1a2 (funcall track)))
          ;; Up to T1a
          (org-up-heading-safe)
          (let ((at-t1a (funcall track)))
            ;; Forward to T1b
            (org-forward-heading-same-level 1)
            (let ((at-t1b (funcall track)))
              ;; Down to T1b1
              (org-next-visible-heading 1)
              (let ((at-t1b1 (funcall track)))
                ;; Jump to T2a1
                (goto-char (point-min))
                (search-forward "T2a1")
                (beginning-of-line)
                (let ((at-t2a1 (funcall track)))
                  ;; Edit: insert T1b2 under T1b
                  (goto-char (point-min))
                  (search-forward "T1b1")
                  (end-of-line)
                  (insert "\n*** T1b2\nBody.\n")
                  ;; Fold T1 subtree
                  (goto-char (point-min))
                  (search-forward "T1")
                  (beginning-of-line)
                  (org-fold-subtree)
                  (let ((after-fold (funcall track)))
                    (list at-t1a1 at-t1a2 at-t1a at-t1b at-t1b1 at-t2a1
                          after-fold
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_twelve_heading_tree_fold_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* P1\n")
    (insert "** P1A\n")
    (insert "*** P1A1\nBody.\n\n")
    (insert "*** P1A2\nBody.\n\n")
    (insert "** P1B\n")
    (insert "*** P1B1\nBody.\n\n")
    (insert "* P2\n")
    (insert "** P2A\n")
    (insert "*** P2A1\nBody.\n\n")
    (insert "** P2B\n")
    (insert "*** P2B1\nBody.\n\n")
    (insert "** P2C\n")
    (insert "*** P2C1\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At P1A1
      (goto-char (point-min))
      (search-forward "P1A1")
      (beginning-of-line)
      (let ((at-p1a1 (funcall track)))
        ;; Forward to P1A2
        (org-forward-heading-same-level 1)
        (let ((at-p1a2 (funcall track)))
          ;; Up to P1A
          (org-up-heading-safe)
          (let ((at-p1a (funcall track)))
            ;; Forward to P1B
            (org-forward-heading-same-level 1)
            (let ((at-p1b (funcall track)))
              ;; Down to P1B1
              (org-next-visible-heading 1)
              (let ((at-p1b1 (funcall track)))
                ;; Jump to P2C1
                (goto-char (point-min))
                (search-forward "P2C1")
                (beginning-of-line)
                (let ((at-p2c1 (funcall track)))
                  ;; Edit: insert P1B2 under P1B
                  (goto-char (point-min))
                  (search-forward "P1B1")
                  (end-of-line)
                  (insert "\n*** P1B2\nBody.\n")
                  ;; Fold P1 subtree
                  (goto-char (point-min))
                  (search-forward "P1")
                  (beginning-of-line)
                  (org-fold-subtree)
                  (let ((after-fold (funcall track)))
                    (list at-p1a1 at-p1a2 at-p1a at-p1b at-p1b1 at-p2c1
                          after-fold
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))))))))))"##,
        expect,
    );
}

#[test]
fn org_navigate_fourteen_heading_tree_fold_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Q1\n")
    (insert "** Q1A\n")
    (insert "*** Q1A1\nBody.\n\n")
    (insert "*** Q1A2\nBody.\n\n")
    (insert "** Q1B\n")
    (insert "*** Q1B1\nBody.\n\n")
    (insert "*** Q1B2\nBody.\n\n")
    (insert "* Q2\n")
    (insert "** Q2A\n")
    (insert "*** Q2A1\nBody.\n\n")
    (insert "** Q2B\n")
    (insert "*** Q2B1\nBody.\n\n")
    (insert "** Q2C\n")
    (insert "*** Q2C1\nBody.\n\n")
    (let ((track (lambda ()
                   (list (line-number-at-pos)
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         (org-outline-level)
                         (invisible-p (point))))))
      ;; At Q1A1
      (goto-char (point-min))
      (search-forward "Q1A1")
      (beginning-of-line)
      (let ((at-q1a1 (funcall track)))
        ;; Forward to Q1A2
        (org-forward-heading-same-level 1)
        (let ((at-q1a2 (funcall track)))
          ;; Up to Q1A
          (org-up-heading-safe)
          (let ((at-q1a (funcall track)))
            ;; Forward to Q1B
            (org-forward-heading-same-level 1)
            (let ((at-q1b (funcall track)))
              ;; Down to Q1B1
              (org-next-visible-heading 1)
              (let ((at-q1b1 (funcall track)))
                ;; Forward to Q1B2
                (org-forward-heading-same-level 1)
                (let ((at-q1b2 (funcall track)))
                  ;; Jump to Q2C1
                  (goto-char (point-min))
                  (search-forward "Q2C1")
                  (beginning-of-line)
                  (let ((at-q2c1 (funcall track)))
                    ;; Edit: insert Q1B3 under Q1B
                    (goto-char (point-min))
                    (search-forward "Q1B2")
                    (end-of-line)
                    (insert "\n*** Q1B3\nBody.\n")
                    ;; Fold Q1 subtree
                    (goto-char (point-min))
                    (search-forward "Q1")
                    (beginning-of-line)
                    (org-fold-subtree)
                    (let ((after-fold (funcall track)))
                      (list at-q1a1 at-q1a2 at-q1a at-q1b at-q1b1 at-q1b2 at-q2c1
                            after-fold
                            (buffer-substring-no-properties
                             (point-min) (point-max))))))))))))))))"##,
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
