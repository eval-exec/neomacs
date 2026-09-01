use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_fold_core_region_copy_narrow_edit_recovery_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable visibility)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (require 'org-fold-core)
  (with-temp-buffer
    (let ((org-cycle-include-plain-lists 'integrate)
          (org-cycle-hide-drawer-startup t)
          (org-cycle-hide-block-startup t)
          (org-fold-show-context-detail
           '((default . lineage)
             (isearch . lineage)
             (bookmark-jump . lineage)))))
      (org-mode)
      (insert "#+STARTUP: content hideblocks\n")
      (insert "* Alpha\n")
      (insert ":PROPERTIES:\n:VISIBILITY: folded\n:Owner: Ada\n:END:\n")
      (insert "alpha body one\n\n")
      (insert "- [ ] parent\n")
      (insert "  - [X] child\n")
      (insert "  - [ ] hidden child\n\n")
      (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
      (insert "** Beta\n")
      (insert ":LOGBOOK:\nCLOCK: [2026-05-27 Wed 08:00]--[2026-05-27 Wed 09:15] =>  1:15\n:END:\n")
      (insert "beta body\n")
      (insert "*** Gamma\n")
      (insert "gamma body\n")
      (insert "**** Delta\n")
      (insert "delta body\n")
      (insert "** Epsilon\n")
      (insert "epsilon body\n")
      (insert "* Zeta\nzeta body\n")
      (let ((needles
             '("Alpha" ":Owner:" "alpha body one" "parent" "child"
               "(+ 1 2)" "Beta" "CLOCK:" "beta body" "Gamma"
               "gamma body" "Delta" "delta body" "Epsilon"
               "epsilon body" "Zeta" "zeta body"))
            states)
        (let ((fold-regions
               (lambda ()
                 (sort
                  (mapcar (lambda (region)
                            (list (nth 0 region)
                                  (nth 1 region)
                                  (nth 2 region)
                                  (buffer-substring-no-properties
                                   (max (point-min) (nth 0 region))
                                   (min (point-max) (nth 1 region)))))
                          (org-fold-core-get-regions
                           :specs '(org-fold-outline
                                    org-fold-drawer
                                    org-fold-block)
                           :from (point-min)
                           :to (point-max)
                           :relative t))
                  (lambda (a b)
                    (if (= (car a) (car b))
                        (string< (symbol-name (nth 2 a))
                                 (symbol-name (nth 2 b)))
                      (< (car a) (car b)))))))
              (visibility
               (lambda ()
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward needle)
                      (list needle
                            (line-number-at-pos)
                            (current-column)
                            (invisible-p (point))
                            (get-text-property (point) 'invisible)
                            (org-fold-get-region-at-point
                             '(headline drawer block)
                             (point)))))
                  needles)))
              (snapshot
               (lambda (label)
                 (let ((visible-copy
                        (progn
                          (org-copy-visible (point-min) (point-max))
                          (current-kill 0 t))))
                   (list label
                         org-cycle-global-status
                         org-cycle-subtree-status
                         (funcall visibility)
                         (funcall fold-regions)
                         (split-string visible-copy "\n" t)
                         (count-lines (point-min) (point-max))
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))
          (org-cycle-set-startup-visibility)
          (org-fold-hide-drawer-all)
          (org-fold-hide-block-all)
          (push (funcall snapshot 'startup) states)
          (goto-char (point-min))
          (search-forward "parent")
          (beginning-of-line)
          (dotimes (_ 3)
            (org-cycle)
            (push (funcall snapshot 'plain-list-cycle) states))
          (goto-char (point-min))
          (search-forward "Beta")
          (beginning-of-line)
          (org-fold-hide-subtree)
          (push (funcall snapshot 'beta-hidden) states)
          (save-restriction
            (org-narrow-to-subtree)
            (goto-char (point-min))
            (search-forward "Gamma")
            (beginning-of-line)
            (org-fold-show-subtree)
            (org-fold-hide-drawer-all)
            (push (list 'narrowed
                        (point-min)
                        (point-max)
                        (funcall visibility)
                        (funcall fold-regions)
                        (buffer-substring-no-properties
                         (point-min) (point-max)))
                  states))
          (push (funcall snapshot 'after-widen) states)
          (goto-char (point-min))
          (search-forward "delta body")
          (org-fold-show-context 'isearch)
          (push (funcall snapshot 'delta-revealed) states)
          (goto-char (point-min))
          (search-forward "Delta")
          (beginning-of-line)
          (org-fold-hide-subtree)
          (end-of-line)
          (insert "\n**** Delta sibling after hidden\nsibling body\n")
          (push (funcall snapshot 'after-hidden-insert) states)
          (org-fold-show-all '(headlines drawers blocks))
          (push (funcall snapshot 'final-show-all) states)
          (list (nreverse states)
                (split-string
                 (buffer-substring-no-properties
                  (point-min) (point-max))
                 "\n" t))))))"##,
        expect,
    );
}

#[test]
fn org_fold_context_narrow_subtree_drawer_block_recovery_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((overview ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" 2 nil) (\"Sibling body\" 2 nil))) (isearch-sixth ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" nil nil) (\"Alpha sixth body\" nil nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" 2 nil) (\"Sibling body\" 2 nil))) (default-beta-fourth ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" nil nil) (\"Beta fourth body\" nil nil) (\"Sibling\" 2 nil) (\"Sibling body\" 2 nil))) (agenda-alpha-child ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" nil nil) (\"Alpha child body\" nil nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" 2 nil) (\"Sibling body\" 2 nil))) (mark-goto-quote ((\"Root\" nil nil) (\"Root paragraph\" nil nil) (\"root quote\" nil nil) (\"Owner\" nil nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" 2 nil) (\"Sibling body\" 2 nil))) (drawers-blocks-hidden ((\"Root\" nil nil) (\"Root paragraph\" nil nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" nil nil) (\"Alpha body\" nil nil) (\"Alpha child\" nil nil) (\"Alpha child body\" nil nil) (\"Alpha fourth\" nil nil) (\"Alpha fourth body\" nil nil) (\"Alpha fifth\" nil nil) (\"Alpha fifth body\" nil nil) (\"Alpha sixth\" nil nil) (\"Alpha sixth body\" nil nil) (\"Beta\" nil nil) (\"Beta body\" nil nil) (\"Beta child\" nil nil) (\"Beta child body\" nil nil) (\"Beta fourth\" nil nil) (\"Beta fourth body\" nil nil) (\"Sibling\" nil nil) (\"Sibling body\" nil nil))) (narrowed-alpha-hidden ((\"Root\" nil nil) (\"Root paragraph\" nil nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" nil nil) (\"Beta body\" nil nil) (\"Beta child\" nil nil) (\"Beta child body\" nil nil) (\"Beta fourth\" nil nil) (\"Beta fourth body\" nil nil) (\"Sibling\" not-found nil) (\"Sibling body\" not-found nil))) (narrowed-alpha-shown ((\"Root\" nil nil) (\"Root paragraph\" nil nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" nil nil) (\"Alpha body\" nil nil) (\"Alpha child\" nil nil) (\"Alpha child body\" nil nil) (\"Alpha fourth\" nil nil) (\"Alpha fourth body\" nil nil) (\"Alpha fifth\" nil nil) (\"Alpha fifth body\" nil nil) (\"Alpha sixth\" nil nil) (\"Alpha sixth body\" nil nil) (\"Beta\" nil nil) (\"Beta body\" nil nil) (\"Beta child\" nil nil) (\"Beta child body\" nil nil) (\"Beta fourth\" nil nil) (\"Beta fourth body\" nil nil) (\"Sibling\" not-found nil) (\"Sibling body\" not-found nil))) (narrowed-sublevels-2 ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" not-found nil) (\"Sibling body\" not-found nil))) (after-widen ((\"Root\" 2 nil) (\"Root paragraph\" 2 nil) (\"root quote\" 2 nil) (\"Owner\" 2 nil) (\"Alpha\" 2 nil) (\"Alpha body\" 2 nil) (\"Alpha child\" 2 nil) (\"Alpha child body\" 2 nil) (\"Alpha fourth\" 2 nil) (\"Alpha fourth body\" 2 nil) (\"Alpha fifth\" 2 nil) (\"Alpha fifth body\" 2 nil) (\"Alpha sixth\" 2 nil) (\"Alpha sixth body\" 2 nil) (\"Beta\" 2 nil) (\"Beta body\" 2 nil) (\"Beta child\" 2 nil) (\"Beta child body\" 2 nil) (\"Beta fourth\" 2 nil) (\"Beta fourth body\" 2 nil) (\"Sibling\" nil nil) (\"Sibling body\" nil nil)))) \"* Root\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\nRoot paragraph.\\n#+begin_quote\\nroot quote\\n#+end_quote\\n** Alpha\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:30] =>  0:30\\n:END:\\nAlpha body.\\n*** Alpha child\\nAlpha child body.\\n**** Alpha fourth\\nAlpha fourth body.\\n***** Alpha fifth\\nAlpha fifth body.\\n****** Alpha sixth\\nAlpha sixth body.\\n** Beta\\nBeta body.\\n*** Beta child\\nBeta child body.\\n**** Beta fourth\\nBeta fourth body.\\n* Sibling\\nSibling body.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-fold-show-context-detail
           '((default . lineage)
             (isearch . lineage)
             (occur . ancestors)
             (bookmark-jump . ancestors)
             (agenda . local)
             (mark-goto . lineage)
             (org-goto . ancestors))))
      (org-mode)
      (insert "* Root\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "Root paragraph.\n")
      (insert "#+begin_quote\nroot quote\n#+end_quote\n")
      (insert "** Alpha\n")
      (insert ":LOGBOOK:\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 09:30] =>  0:30\n:END:\n")
      (insert "Alpha body.\n")
      (insert "*** Alpha child\n")
      (insert "Alpha child body.\n")
      (insert "**** Alpha fourth\n")
      (insert "Alpha fourth body.\n")
      (insert "***** Alpha fifth\n")
      (insert "Alpha fifth body.\n")
      (insert "****** Alpha sixth\n")
      (insert "Alpha sixth body.\n")
      (insert "** Beta\n")
      (insert "Beta body.\n")
      (insert "*** Beta child\n")
      (insert "Beta child body.\n")
      (insert "**** Beta fourth\n")
      (insert "Beta fourth body.\n")
      (insert "* Sibling\n")
      (insert "Sibling body.\n")
        (let ((probe
               (lambda (needle)
                 (save-excursion
                   (goto-char (point-min))
                   (if (search-forward needle nil t)
                       (list needle
                             (invisible-p (point))
                             (get-text-property (point) 'invisible))
                     (list needle 'not-found nil)))))
            states)
        (let ((snapshot
               (lambda (label)
                 (push (list label
                             (mapcar probe
                                     '("Root" "Root paragraph" "root quote"
                                       "Owner" "Alpha" "Alpha body"
                                       "Alpha child" "Alpha child body"
                                       "Alpha fourth" "Alpha fourth body"
                                       "Alpha fifth" "Alpha fifth body"
                                       "Alpha sixth" "Alpha sixth body"
                                       "Beta" "Beta body" "Beta child"
                                       "Beta child body" "Beta fourth"
                                       "Beta fourth body" "Sibling"
                                       "Sibling body")))
                       states))))
          (org-fold-hide-sublevels 1)
          (funcall snapshot 'overview)
          (goto-char (point-min))
          (search-forward "Alpha sixth body.")
          (org-fold-show-context 'isearch)
          (funcall snapshot 'isearch-sixth)
          (org-fold-hide-sublevels 1)
          (goto-char (point-min))
          (search-forward "Beta fourth body.")
          (org-fold-show-context 'default)
          (funcall snapshot 'default-beta-fourth)
          (org-fold-hide-sublevels 1)
          (goto-char (point-min))
          (search-forward "Alpha child body.")
          (org-fold-show-context 'agenda)
          (funcall snapshot 'agenda-alpha-child)
          (org-fold-hide-sublevels 1)
          (goto-char (point-min))
          (search-forward "root quote")
          (org-fold-show-context 'mark-goto)
          (funcall snapshot 'mark-goto-quote)
          (org-fold-show-all)
          (org-fold-hide-drawer-all)
          (org-fold-hide-block-all)
          (funcall snapshot 'drawers-blocks-hidden)
          (save-restriction
            (org-narrow-to-subtree)
            (goto-char (point-min))
            (search-forward "Alpha")
            (beginning-of-line)
            (org-fold-hide-subtree)
            (funcall snapshot 'narrowed-alpha-hidden)
            (org-fold-show-subtree)
            (funcall snapshot 'narrowed-alpha-shown)
            (org-fold-hide-sublevels 2)
            (funcall snapshot 'narrowed-sublevels-2))
          (funcall snapshot 'after-widen)
           (list (nreverse states)
                 (buffer-substring-no-properties
                  (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_core_region_spec_visibility_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Owner\" 2 nil) (\"(+ 1 2)\" 2 nil) (\"clock line\" 2 nil) (\"Alpha body\" nil nil) (\"Beta body\" nil nil) (\"Gamma body\" nil nil)) ((\"Beta body\" 2 nil) (\"Gamma body\" 2 nil) (\"Delta body\" nil nil)) ((\"Beta body\" nil nil) (\"Gamma body\" nil nil) (\"Delta body\" nil nil)) ((\"Owner\" nil nil) (\"(+ 1 2)\" nil nil) (\"clock line\" nil nil)) \"* Alpha\\n:PROPERTIES:\\n:Owner: Ada\\n:END:\\nAlpha body.\\n#+begin_src emacs-lisp\\n(+ 1 2)\\n#+end_src\\n** Beta\\n:LOGBOOK:\\nclock line\\n:END:\\nBeta body.\\n*** Gamma\\nGamma body.\\n* Delta\\nDelta body.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
    (insert "Alpha body.\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "** Beta\n")
    (insert ":LOGBOOK:\nclock line\n:END:\n")
    (insert "Beta body.\n")
    (insert "*** Gamma\n")
    (insert "Gamma body.\n")
    (insert "* Delta\n")
    (insert "Delta body.\n")
    (let ((probe (lambda (needle)
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward needle)
                     (list needle
                           (invisible-p (point))
                           (get-text-property (point) 'invisible))))))
      (org-fold-hide-drawer-all)
      (org-fold-hide-block-all)
      (let ((after-db (mapcar probe '("Owner" "(+ 1 2)" "clock line" "Alpha body" "Beta body" "Gamma body"))))
        (goto-char (point-min))
        (search-forward "Beta")
        (beginning-of-line)
        (org-fold-hide-subtree)
        (let ((after-hide (mapcar probe '("Beta body" "Gamma body" "Delta body"))))
          (org-fold-show-subtree)
          (let ((after-show (mapcar probe '("Beta body" "Gamma body" "Delta body"))))
            (org-fold-show-all '(drawers blocks))
            (let ((after-db-show (mapcar probe '("Owner" "(+ 1 2)" "clock line"))))
              (list after-db after-hide after-show after-db-show
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_reveal_context_narrow_widen_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Root\" 1 2 org-fold-outline) (\"root body\" 2 2 org-fold-outline) (\"Alpha\" 3 2 org-fold-outline) (\"Beta\" 5 2 org-fold-outline) (\"Delta\" 9 2 org-fold-outline) (\"Sibling\" 11 2 org-fold-outline)) ((\"Root\" 1 2 org-fold-outline) (\"root body\" 2 nil nil) (\"Alpha\" 3 2 org-fold-outline) (\"alpha body\" 4 nil nil) (\"Beta\" 5 2 org-fold-outline) (\"beta body\" 6 nil nil) (\"Gamma\" 7 2 org-fold-outline) (\"delta body\" 10 nil nil) (\"Sibling\" 11 2 org-fold-outline)) ((\"Root\" 1 2 org-fold-outline) (\"Alpha\" 3 2 org-fold-outline) (\"Beta\" 5 nil nil) (\"beta body\" 6 nil nil) (\"Gamma\" 7 2 org-fold-outline) (\"Sibling\" 11 2 org-fold-outline)) ((\"Alpha\" 1 2 org-fold-outline) (\"alpha body\" 2 2 org-fold-outline) (\"Beta\" 3 2 org-fold-outline) (\"gamma body\" 6 2 org-fold-outline) (\"Sibling\" not-found nil nil)) \"** Alpha\\nalpha body\\n*** Beta\\nbeta body\\n**** Gamma\\ngamma body\\n***** Delta\\ndelta body\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-fold-show-context-detail '((default . lineage)
                                          (isearch . lineage)
                                          (agenda . local))))
      (org-mode)
      (insert "* Root\nroot body\n")
      (insert "** Alpha\nalpha body\n")
      (insert "*** Beta\nbeta body\n")
      (insert "**** Gamma\ngamma body\n")
      (insert "***** Delta\ndelta body\n")
      (insert "** Sibling\nsibling body\n")
      (let ((probe (lambda (needle)
                     (save-excursion
                       (goto-char (point-min))
                       (if (search-forward needle nil t)
                           (list needle
                                 (line-number-at-pos)
                                 (invisible-p (point))
                                 (org-fold-folded-p (point) 'headline))
                         (list needle 'not-found nil nil))))))
        ;; Hide all to level 1
        (org-fold-hide-sublevels 1)
        (let ((after-hide-1 (mapcar probe '("Root" "root body" "Alpha" "Beta" "Delta" "Sibling"))))
          ;; Reveal with isearch context
          (goto-char (point-min))
          (search-forward "delta body")
          (org-fold-show-context 'isearch)
          (let ((after-isearch (mapcar probe '("Root" "root body" "Alpha" "alpha body" "Beta" "beta body" "Gamma" "delta body" "Sibling"))))
            ;; Hide again
            (org-fold-hide-sublevels 1)
            ;; Reveal with agenda context
            (goto-char (point-min))
            (search-forward "beta body")
            (org-fold-show-context 'agenda)
            (let ((after-agenda (mapcar probe '("Root" "Alpha" "Beta" "beta body" "Gamma" "Sibling"))))
              ;; Narrow to subtree
              (goto-char (point-min))
              (search-forward "Alpha")
              (beginning-of-line)
              (save-restriction
                (org-narrow-to-subtree)
                (org-fold-hide-sublevels 1)
                (let ((narrowed-vis (mapcar probe '("Alpha" "alpha body" "Beta" "gamma body" "Sibling"))))
                   (list after-hide-1
                         after-isearch
                         after-agenda
                         narrowed-vis
                         (buffer-substring-no-properties
                          (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_drawer_block_hide_show_all_recovery_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"Owner\" 2 nil) (\"clock line\" 2 nil) (\"(+ 1 2)\" 2 nil) (\"quoted text\" 2 nil) (\"Alpha body\" nil nil) (\"Beta body\" nil nil)) ((\"Owner\" nil nil) (\"clock line\" nil nil) (\"(+ 1 2)\" nil nil) (\"quoted text\" nil nil) (\"Alpha body\" nil nil) (\"Beta body\" nil nil)) ((\"Owner\" 2 nil) (\"clock line\" 2 nil) (\"(+ 1 2)\" 2 nil) (\"quoted text\" 2 nil)) ((\"Owner\" nil nil) (\"clock line\" nil nil) (\"(+ 1 2)\" nil nil) (\"quoted text\" nil nil) (\"Alpha body\" nil nil) (\"Beta body\" nil nil)) \"* Alpha\\n:PROPERTIES:\\n:Owner: Ada\\n:Effort: 1:00\\n:END:\\n:LOGBOOK:\\nclock line\\n:END:\\nAlpha body.\\n#+begin_quote\\nquoted text\\n#+end_quote\\n#+begin_src emacs-lisp\\n(+ 1 2)\\n#+end_src\\n** Beta\\n:PROPERTIES:\\n:Owner: Bob\\n:END:\\nBeta body.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 1:00\n:END:\n")
    (insert ":LOGBOOK:\nclock line\n:END:\n")
    (insert "Alpha body.\n")
    (insert "#+begin_quote\nquoted text\n#+end_quote\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "** Beta\n")
    (insert ":PROPERTIES:\n:Owner: Bob\n:END:\n")
    (insert "Beta body.\n")
    (let ((probe (lambda (needle)
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward needle)
                     (list needle
                           (invisible-p (point))
                           (get-text-property (point) 'invisible))))))
      ;; Hide drawers and blocks
      (org-fold-hide-drawer-all)
      (org-fold-hide-block-all)
      (let ((after-hide (mapcar probe '("Owner" "clock line" "(+ 1 2)" "quoted text" "Alpha body" "Beta body"))))
        ;; Show all drawers and blocks
        (org-fold-show-all '(drawers blocks))
        (let ((after-show (mapcar probe '("Owner" "clock line" "(+ 1 2)" "quoted text" "Alpha body" "Beta body"))))
          ;; Hide again
          (org-fold-hide-drawer-all)
          (org-fold-hide-block-all)
          (let ((after-hide-again (mapcar probe '("Owner" "clock line" "(+ 1 2)" "quoted text"))))
            ;; Show all
            (org-fold-show-all)
            (let ((after-show-all (mapcar probe '("Owner" "clock line" "(+ 1 2)" "quoted text" "Alpha body" "Beta body"))))
              (list after-hide
                    after-show
                    after-hide-again
                    after-show-all
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_fold_subtree_edit_hidden_body_reveal_font_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 64 45)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha\n")
    (insert "Alpha body.\n")
    (insert "** Beta\n")
    (insert "Beta body.\n")
    (insert "*** Gamma\n")
    (insert "Gamma body.\n")
    (insert "**** Delta\n")
    (insert "Delta body.\n")
    (insert "* Sibling\n")
    (insert "Sibling body.\n")
    ;; Hide Beta subtree
    (goto-char (point-min))
    (search-forward "Beta")
    (beginning-of-line)
    (org-fold-hide-subtree)
    ;; Edit while hidden: insert after Beta heading
    (end-of-line)
    (insert "\n** Inserted after Beta\nInserted body.\n")
    ;; Check state after hidden edit
    (let ((after-edit
           (mapcar
            (lambda (needle)
              (save-excursion
                (goto-char (point-min))
                (if (search-forward needle nil t)
                    (list needle
                          (line-number-at-pos)
                          (invisible-p (point))
                          (org-outline-level))
                    (list needle 'not-found nil nil))))
            '("Alpha" "Beta" "Gamma" "Delta" "Inserted" "Sibling"))))
      ;; Show all
      (org-fold-show-all)
      ;; Check final state
      (let ((after-show
             (mapcar
              (lambda (needle)
                (save-excursion
                  (goto-char (point-min))
                  (if (search-forward needle nil t)
                      (list needle
                            (line-number-at-pos)
                            (invisible-p (point))
                            (org-outline-level))
                      (list needle 'not-found nil nil))))
              '("Alpha" "Beta" "Gamma" "Delta" "Inserted" "Sibling")))
            ;; Check merged
            (merged nil))
        (dolist (line (split-string
                       (buffer-substring-no-properties
                        (point-min) (point-max))
                       "\n" t))
          (when (string-match-p "^\\*+ .*\\*+ " line)
            (push line merged)))
        (list after-edit
              after-show
              (nreverse merged)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}
