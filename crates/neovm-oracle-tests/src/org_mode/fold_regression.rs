use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_fold_indirect_deep_edit_font_lock_regression_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable visibility)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'org-fold-core)
  (with-temp-buffer
    (let ((org-cycle-global-at-bob t)
          (org-cycle-level-faces t)
          (org-fontify-whole-heading-line t)
          (org-fontify-done-headline t)
          (org-fontify-todo-headline t)
          (org-cycle-separator-lines 0)
          (org-startup-folded 'showeverything))
      (org-mode)
      (insert "* TODO Root\n")
      (insert "Root body.\n")
      (insert "** TODO Area A\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "Area A body.\n")
      (insert "*** TODO Thread A1\n")
      (insert "Thread A1 body.\n")
      (insert "**** TODO Fourth A1\n")
      (insert "Fourth A1 body.\n")
      (insert "***** WAIT Fifth A1\n")
      (insert "Fifth A1 body.\n")
      (insert "****** DONE Sixth A1\n")
      (insert "Sixth A1 body.\n")
      (insert "** TODO Area B\n")
      (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
      (insert "*** NEXT Thread B1\n")
      (insert "**** TODO Fourth B1\n")
      (insert "Fourth B1 body.\n")
      (insert "***** TODO Fifth B1\n")
      (insert "Fifth B1 body.\n")
      (insert "* Tail\nTail body.\n")
      (let ((needles
             '("Root" "Root body" "Area A" ":Owner:" "Thread A1"
               "Fourth A1" "Fourth A1 body" "Fifth A1" "Fifth A1 body"
               "Sixth A1" "Sixth A1 body" "Area B" "(+ 1 2)"
               "Fourth B1" "Fourth B1 body" "Fifth B1" "Tail body"))
            clone states)
        (let ((headings
               (lambda ()
                 (font-lock-ensure (point-min) (point-max))
                 (let (out)
                   (goto-char (point-min))
                   (while (re-search-forward "^\\(\\*+\\) +\\(.*\\)$" nil t)
                     (let ((beg (line-beginning-position)))
                       (push (list (match-string 1)
                                   (match-string 2)
                                   (org-outline-level)
                                   (get-text-property beg 'face)
                                   (get-text-property
                                    (match-beginning 2) 'face)
                                   (get-text-property beg 'invisible)
                                   (org-fold-folded-p beg 'headline))
                             out)))
                   (nreverse out))))
              (visibility
               (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward needle)
                      (let ((pos (match-beginning 0)))
                        (list needle
                              (line-number-at-pos pos)
                              (current-column)
                              (invisible-p pos)
                              (get-text-property pos 'invisible)
                              (get-text-property pos 'face)))))
                  needles)))
              (fold-regions
               (lambda ()
                 (sort
                  (mapcar (lambda (region)
                            (list (nth 0 region)
                                  (nth 1 region)
                                  (nth 2 region)))
                          (org-fold-core-get-regions
                           :specs '(org-fold-outline
                                    org-fold-drawer
                                    org-fold-block)
                           :from (point-min)
                           :to (point-max)
                           :relative t))
                  (lambda (a b)
                    (if (= (car a) (car b))
                        (< (nth 1 a) (nth 1 b))
                      (< (car a) (car b)))))))
              (snapshot
               (lambda (label)
                 (list label
                       org-cycle-global-status
                       org-cycle-subtree-status
                       (funcall visibility)
                       (funcall headings)
                       (funcall fold-regions)
                       (split-string
                        (buffer-substring-no-properties
                         (point-min) (point-max))
                        "\n" t)))))
          (unwind-protect
              (progn
                (font-lock-ensure (point-min) (point-max))
                (org-fold-hide-sublevels 2)
                (org-fold-hide-drawer-all)
                (org-fold-hide-block-all)
                (push (funcall snapshot 'base-initial-hidden) states)
                (setq clone (clone-indirect-buffer nil nil))
                (with-current-buffer clone
                  (org-fold-core-decouple-indirect-buffer-folds)
                  (goto-char (point-min))
                  (search-forward "Fourth A1")
                  (beginning-of-line)
                  (org-fold-show-subtree)
                  (search-forward "Fifth A1")
                  (beginning-of-line)
                  (org-fold-hide-subtree)
                  (end-of-line)
                  (insert "\n***** TODO Inserted while fifth hidden\nInserted body.\n")
                  (push (funcall snapshot 'clone-after-hidden-insert) states)
                  (goto-char (point-min))
                  (search-forward "Area B")
                  (beginning-of-line)
                  (dotimes (_ 4)
                    (org-cycle)
                    (push (funcall snapshot 'clone-area-b-cycle) states)))
                (push (funcall snapshot 'base-after-clone-cycles) states)
                (goto-char (point-min))
                (dotimes (_ 5)
                  (org-cycle-global)
                  (push (funcall snapshot 'base-global-cycle) states))
                (org-fold-show-all)
                (font-lock-ensure (point-min) (point-max))
                (let ((merged nil)
                      (bad-levels nil))
                  (dolist (line (split-string
                                 (buffer-substring-no-properties
                                  (point-min) (point-max))
                                 "\n" t))
                    (when (string-match-p "^\\*+ .*\\*+ " line)
                      (push line merged)))
                  (goto-char (point-min))
                  (while (re-search-forward "^\\(\\*+\\) +\\(.*\\)$" nil t)
                    (let ((stars (length (match-string 1)))
                          (level (org-outline-level)))
                      (unless (= stars level)
                        (push (list (match-string 0) stars level)
                              bad-levels))))
                  (push (funcall snapshot 'base-final-show-all) states)
                  (list (nreverse states)
                        (nreverse merged)
                        (nreverse bad-levels)
                        (mapcar (lambda (needle)
                                  (save-excursion
                                    (goto-char (point-min))
                                    (search-forward needle nil t)))
                                '("***** TODO Inserted while fifth hidden"
                                  "Inserted body."
                                  "****** DONE Sixth A1"
                                  "***** TODO Fifth B1"))
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))
            (when (buffer-live-p clone)
               (kill-buffer clone))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_startup_content_drawer_deep_cycle_hidden_edit_regression_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'org-fold-core)
  (with-temp-buffer
    (let ((org-cycle-global-at-bob t)
          (org-cycle-level-faces t)
          (org-fontify-whole-heading-line t)
          (org-fontify-done-headline t)
          (org-fontify-todo-headline t)
          (org-hide-emphasis-markers t)
          (org-link-descriptive t)
          (org-cycle-hide-drawer-startup t)
          (org-cycle-hide-block-startup t)
          (org-cycle-separator-lines 0)
          (org-startup-folded 'showeverything))
      (org-mode)
      (insert "#+STARTUP: content\n")
      (insert "* TODO Project :root:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 5:00\n:END:\n")
      (insert "Project intro with *bold* and /italic/.\n")
      (insert "** NEXT Alpha :work:\n")
      (insert ":LOGBOOK:\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:30] =>  0:30\n:END:\n")
      (insert "Alpha body paragraph.\n")
      (insert "*** WAIT Alpha child :deep:\n")
      (insert "#+begin_src emacs-lisp\n(message \"hello\")\n(+ 1 2)\n#+end_src\n")
      (insert "**** TODO Fourth level :level4:\n")
      (insert "Fourth body.\n")
      (insert "***** DONE Fifth level\n")
      (insert "Fifth body.\n")
      (insert "****** TODO Sixth level :level6:\n")
      (insert "Sixth body.\n")
      (insert "******* WAIT Seventh level\n")
      (insert "Seventh body.\n")
      (insert "** TODO Beta :work:\n")
      (insert "Beta body.\n")
      (insert "*** NEXT Beta child\n")
      (insert "Beta child body.\n")
      (insert "**** TODO Beta fourth\n")
      (insert "Beta fourth body.\n")
      (insert "***** TODO Beta fifth\n")
      (insert "Beta fifth body.\n")
      (insert "* DONE Tail :done:\n")
      (insert "CLOSED: [2026-05-27 Wed]\n")
      (insert "Tail body.\n")
      (let ((needles
             '("Project" ":Owner:" "Project intro" "Alpha"
               "Alpha body" "Alpha child" "(message" "(+ 1 2)"
               "Fourth level" "Fourth body" "Fifth level" "Fifth body"
               "Sixth level" "Sixth body" "Seventh level" "Seventh body"
               "Beta" "Beta body" "Beta child" "Beta child body"
               "Beta fourth" "Beta fourth body" "Beta fifth"
               "Beta fifth body" "Tail" "Tail body"))
            states)
        (let ((snapshot
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
                            (let ((pos (match-beginning 0)))
                              (list needle
                                    (line-number-at-pos pos)
                                    (invisible-p pos)
                                    (get-text-property pos 'invisible)
                                    (get-text-property
                                     (line-beginning-position) 'face)
                                    (get-text-property pos 'face)))))
                        needles)
                       (save-excursion
                         (goto-char (point-min))
                         (let (out)
                           (while (re-search-forward "^\\(\\*+\\) +\\(.*\\)$" nil t)
                             (push (list (match-string 1)
                                         (match-string 2)
                                         (org-outline-level)
                                         (get-text-property
                                          (line-beginning-position) 'face)
                                         (get-text-property
                                          (match-beginning 2) 'face))
                                   out))
                           (nreverse out)))
                       (count-matches "^\\*+ " (point-min) (point-max))
                       (count-lines (point-min) (point-max))
                       (split-string
                        (buffer-substring-no-properties
                         (point-min) (point-max))
                        "\n" t)))))
          (org-cycle-set-startup-visibility)
          (push (funcall snapshot 'startup-content) states)
          (org-fold-hide-drawer-all)
          (org-fold-hide-block-all)
          (push (funcall snapshot 'drawers-blocks-hidden) states)
          (goto-char (point-min))
          (search-forward "Fourth level")
          (beginning-of-line)
          (dotimes (_ 5)
            (org-cycle)
            (push (funcall snapshot 'cycle-fourth) states))
          (goto-char (point-min))
          (search-forward "Sixth level")
          (beginning-of-line)
          (org-fold-hide-subtree)
          (push (funcall snapshot 'sixth-hidden) states)
          (end-of-line)
          (insert "\n****** TODO Inserted while sixth hidden\nInserted sixth body.\n")
          (push (funcall snapshot 'after-hidden-insert) states)
          (goto-char (point-min))
          (search-forward "Beta")
          (beginning-of-line)
          (dotimes (_ 4)
            (org-cycle)
            (push (funcall snapshot 'cycle-beta) states))
          (goto-char (point-min))
          (search-forward "Inserted sixth body.")
          (org-next-visible-heading 1)
          (push (funcall snapshot 'next-visible-after-insert) states)
          (goto-char (point-min))
          (search-forward "Beta fifth body.")
          (org-fold-show-context 'isearch)
          (push (funcall snapshot 'context-beta-fifth) states)
          (goto-char (point-min))
          (dotimes (_ 6)
            (org-cycle-global)
            (push (funcall snapshot 'global-cycle) states))
          (org-fold-show-all)
          (font-lock-ensure (point-min) (point-max))
          (let ((merged nil)
                (bad-levels nil))
            (dolist (line (split-string
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           "\n" t))
              (when (string-match-p "^\\*+ .*\\*+ " line)
                (push line merged)))
            (goto-char (point-min))
            (while (re-search-forward "^\\(\\*+\\) +\\(.*\\)$" nil t)
              (let ((stars (length (match-string 1)))
                    (level (org-outline-level)))
                (unless (= stars level)
                  (push (list (match-string 0) stars level)
                        bad-levels))))
            (push (funcall snapshot 'final-show-all) states)
            (list (nreverse states)
                  (nreverse merged)
                  (nreverse bad-levels)
                  (mapcar
                   (lambda (needle)
                     (save-excursion
                       (goto-char (point-min))
                       (search-forward needle nil t)))
                   '("****** TODO Inserted while sixth hidden"
                     "Inserted sixth body."
                     "******* WAIT Seventh level"
                     "***** TODO Beta fifth"))
                   (buffer-substring-no-properties
                    (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_indirect_buffer_decouple_edit_font_regression_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil nil \"* TODO Root\\nRoot body.\\n** TODO Alpha\\nAlpha body.\\n*** TODO Alpha child\\nAlpha child body.\\n**** TODO Alpha L4\\nAlpha L4 body.\\n***** DONE Alpha L5\\n***** TODO Inserted under hidden L5\\nInserted body.\\n\\nAlpha L5 body.\\n** TODO Beta\\nBeta body.\\n* Tail\\nTail body.\\n\" \"* TODO Root\\nRoot body.\\n** TODO Alpha\\nAlpha body.\\n*** TODO Alpha child\\nAlpha child body.\\n**** TODO Alpha L4\\nAlpha L4 body.\\n***** DONE Alpha L5\\n***** TODO Inserted under hidden L5\\nInserted body.\\n\\nAlpha L5 body.\\n** TODO Beta\\nBeta body.\\n* Tail\\nTail body.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'org-fold-core)
  (with-temp-buffer
    (let ((org-cycle-global-at-bob t)
          (org-cycle-level-faces t)
          (org-fontify-whole-heading-line t)
          (org-startup-folded 'showeverything))
      (org-mode)
      (insert "* TODO Root\n")
      (insert "Root body.\n")
      (insert "** TODO Alpha\n")
      (insert "Alpha body.\n")
      (insert "*** TODO Alpha child\n")
      (insert "Alpha child body.\n")
      (insert "**** TODO Alpha L4\n")
      (insert "Alpha L4 body.\n")
      (insert "***** DONE Alpha L5\n")
      (insert "Alpha L5 body.\n")
      (insert "** TODO Beta\n")
      (insert "Beta body.\n")
      (insert "* Tail\n")
      (insert "Tail body.\n")
      (font-lock-ensure (point-min) (point-max))
      (org-fold-hide-sublevels 2)
      ;; Create indirect buffer
      (let ((clone (clone-indirect-buffer nil nil)))
        (with-current-buffer clone
          (org-fold-core-decouple-indirect-buffer-folds)
          ;; Show subtree in clone
          (goto-char (point-min))
          (search-forward "Alpha")
          (beginning-of-line)
          (org-fold-show-subtree)
          ;; Hide L5 in clone
          (search-forward "Alpha L5")
          (beginning-of-line)
          (org-fold-hide-subtree)
          ;; Edit while L5 hidden
          (end-of-line)
          (insert "\n***** TODO Inserted under hidden L5\nInserted body.\n")
          (font-lock-ensure (point-min) (point-max))
          ;; Check for merged headings
          (let ((merged nil)
                (bad-levels nil))
            (dolist (line (split-string
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           "\n" t))
              (when (string-match-p "^\\*+ .*\\*+ " line)
                (push line merged)))
            (goto-char (point-min))
            (while (re-search-forward "^\\(\\*+\\) +\\(.*\\)$" nil t)
              (let ((stars (length (match-string 1)))
                    (level (org-outline-level)))
                (unless (= stars level)
                  (push (list (match-string 0) stars level)
                        bad-levels))))
            ;; Global cycles
            (goto-char (point-min))
            (dotimes (_ 5) (org-cycle-global))
            (org-fold-show-all)
            (font-lock-ensure (point-min) (point-max))
            (let ((clone-final (buffer-substring-no-properties
                                (point-min) (point-max))))
              ;; Check base buffer
              (with-current-buffer (buffer-base-buffer)
                (let ((base-final (buffer-substring-no-properties
                                   (point-min) (point-max))))
                  (kill-buffer clone)
                  (list (nreverse merged)
                        (nreverse bad-levels)
                        (search-forward "Inserted under hidden L5" nil t)
                        clone-final
                        base-final))))))))))"##,
        expect,
    );
}
