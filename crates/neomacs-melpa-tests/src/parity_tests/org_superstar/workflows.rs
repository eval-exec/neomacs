use expect_test::expect;

use super::ParityBatchCase;

fn default_headlines_cycle_bullets_and_prettify_leading_stars() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_headlines_cycle_bullets_and_prettify_leading_stars",
        r##"
(with-temp-buffer
  (neomacs-org-superstar-test-fontify
   "* Plan\n** Build\n*** Test\n**** Release\n***** Observe\n")
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :headings (neomacs-org-superstar-test-headings)
        :mode org-superstar-mode
        :invisibility (member '(org-superstar-hide) buffer-invisibility-spec)
        :pre-hook (and (memq #'org-superstar--invisibility-off
                             pre-command-hook)
                       t)
        :post-hook (and (memq #'org-superstar--invisibility-on
                              post-command-hook)
                        t)))
"##,
        expect![[
            r#"OK (:text "* Plan\n** Build\n*** Test\n**** Release\n***** Observe\n" :headings ((:raw "* Plan" :level 1 :leading nil :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "** Build" :level 2 :leading ((:composition "((1 . 8229))" :face org-superstar-leading :invisible nil)) :bullet (:composition "((1 . 9675))" :face (org-superstar-header-bullet org-level-2) :invisible nil)) (:raw "*** Test" :level 3 :leading ((:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil)) :bullet (:composition "((1 . 10040))" :face (org-superstar-header-bullet org-level-3) :invisible nil)) (:raw "**** Release" :level 4 :leading ((:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil)) :bullet (:composition "((1 . 10047))" :face (org-superstar-header-bullet org-level-4) :invisible nil)) (:raw "***** Observe" :level 5 :leading ((:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil) (:composition "((1 . 8229))" :face org-superstar-leading :invisible nil)) :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-5) :invisible nil))) :mode t :invisibility ((org-superstar-hide) (org-babel-hide-result . t) (org-hide-block . t) (org-fold-outline . "...") (org-hide-block . "...") (org-hide-drawer . "...") (org-link) (outline . t) t) :pre-hook t :post-hook t)"#
        ]],
    )
}

fn custom_cycle_rules_and_terminal_fallbacks_select_exact_bullets() -> ParityBatchCase {
    ParityBatchCase::value(
        "custom_cycle_rules_and_terminal_fallbacks_select_exact_bullets",
        r##"
(let ((org-superstar-headline-bullets-list
       '(?A "B" ("compose-C" ?C) nil))
      (org-superstar-leading-fallback ?.)
      (org-superstar-first-inlinetask-fallback ?F))
  (list
   :all (let ((org-superstar-cycle-headline-bullets t))
          (mapcar #'org-superstar-hbullet '(1 2 3 4 5 6)))
   :repeat-last (let ((org-superstar-cycle-headline-bullets nil))
                  (mapcar #'org-superstar-hbullet '(1 2 3 4 5 6)))
   :first-two (let ((org-superstar-cycle-headline-bullets 2))
                (mapcar #'org-superstar-hbullet '(1 2 3 4 5 6)))
   :last-two (let ((org-superstar-cycle-headline-bullets -2))
               (mapcar #'org-superstar-hbullet '(1 2 3 4 5 6)))
   :leading (org-superstar-lbullet)
   :inline-first (org-superstar-fbullet)))
"##,
        expect![
            "OK (:all (65 66 67 nil 65 66) :repeat-last (65 66 67 nil nil nil) :first-two (65 66 65 66 65 66) :last-two (65 66 67 nil 67 nil) :leading 46 :inline-first 70)"
        ],
    )
}

fn todo_bullets_support_exact_default_and_hidden_keyword_policies() -> ParityBatchCase {
    ParityBatchCase::value(
        "todo_bullets_support_exact_default_and_hidden_keyword_policies",
        r##"
(let ((org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE")))
      (org-superstar-special-todo-items t)
      (org-superstar-todo-bullet-alist
       '(("TODO" . ?T) ("DONE" . ?D) (default . ?X))))
  (with-temp-buffer
    (neomacs-org-superstar-test-fontify
     "* TODO Ship\n* WAIT Review\n* DONE Close\n* Plain\n")
    (let ((rendered (neomacs-org-superstar-test-headings))
          hidden)
      (setq org-superstar-special-todo-items 'hide)
      (org-superstar-restart)
      (font-lock-ensure (point-min) (point-max))
      (setq hidden (neomacs-org-superstar-test-headings))
      (list :rendered rendered :hidden hidden))))
"##,
        expect![[
            r#"OK (:rendered ((:raw "* TODO Ship" :level 1 :leading nil :bullet (:composition "((1 . 84))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "* WAIT Review" :level 1 :leading nil :bullet (:composition "((1 . 88))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "* DONE Close" :level 1 :leading nil :bullet (:composition "((1 . 68))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "* Plain" :level 1 :leading nil :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-1) :invisible nil))) :hidden ((:raw "* TODO Ship" :level 1 :leading nil :bullet (:composition nil :face (org-superstar-header-bullet org-level-1) :invisible org-superstar-hide)) (:raw "* WAIT Review" :level 1 :leading nil :bullet (:composition nil :face (org-superstar-header-bullet org-level-1) :invisible org-superstar-hide)) (:raw "* DONE Close" :level 1 :leading nil :bullet (:composition nil :face (org-superstar-header-bullet org-level-1) :invisible org-superstar-hide)) (:raw "* Plain" :level 1 :leading nil :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-1) :invisible nil))))"#
        ]],
    )
}

fn real_plain_lists_are_prettified_but_source_block_lookalikes_are_not() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_plain_lists_are_prettified_but_source_block_lookalikes_are_not",
        r##"
(let ((org-list-allow-alphabetical t)
      (org-superstar-item-bullet-alist
       '((?* . ?S) (?+ . ?P) (?- . ?M))))
  (with-temp-buffer
    (neomacs-org-superstar-test-fontify
     (concat
      "* Release\n"
      " * starred\n"
      "   + plus\n"
      "     - minus\n"
      "1. first\n"
      "a) alpha\n"
      "#+begin_src text\n"
      " * not-a-list\n"
      " + not-a-list\n"
      "#+end_src\n"))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :items (neomacs-org-superstar-test-list-state))))
"##,
        expect![[
            r#"OK (:text "* Release\n * starred\n   + plus\n     - minus\n1. first\na) alpha\n#+begin_src text\n * not-a-list\n + not-a-list\n#+end_src\n" :items ((:line 1 :indent 0 :raw "*" :display nil :face (org-superstar-header-bullet org-level-1)) (:line 2 :indent 1 :raw "*" :display "S" :face org-superstar-item) (:line 3 :indent 3 :raw "+" :display "P" :face org-superstar-item) (:line 4 :indent 5 :raw "-" :display "M" :face org-superstar-item) (:line 5 :indent 0 :raw "1." :display nil :face org-superstar-ordered-item) (:line 6 :indent 0 :raw "a)" :display nil :face org-superstar-ordered-item) (:line 8 :indent 1 :raw "*" :display nil :face #1=(org-block)) (:line 9 :indent 1 :raw "+" :display nil :face #1#)))"#
        ]],
    )
}

fn lightweight_lists_intentionally_prettify_source_block_lookalikes() -> ParityBatchCase {
    ParityBatchCase::value(
        "lightweight_lists_intentionally_prettify_source_block_lookalikes",
        r##"
(with-temp-buffer
  (insert "#+begin_src text\n * fake\n#+end_src\n")
  (org-mode)
  (org-superstar-toggle-lightweight-lists)
  (org-superstar-mode 1)
  (font-lock-ensure (point-min) (point-max))
  (let ((enabled (list :lightweight org-superstar-lightweight-lists
                       :items (neomacs-org-superstar-test-list-state))))
    (org-superstar-toggle-lightweight-lists)
    (org-superstar-restart)
    (font-lock-ensure (point-min) (point-max))
    (list :enabled enabled
          :disabled
          (list :lightweight org-superstar-lightweight-lists
                :items (neomacs-org-superstar-test-list-state)))))
"##,
        expect![[
            r#"OK (:enabled (:lightweight t :items ((:line 2 :indent 1 :raw "*" :display "•" :face (org-block)))) :disabled (:lightweight nil :items ((:line 2 :indent 1 :raw "*" :display nil :face (org-block)))))"#
        ]],
    )
}

fn headline_hook_can_add_user_visible_properties_at_each_heading() -> ParityBatchCase {
    ParityBatchCase::value(
        "headline_hook_can_add_user_visible_properties_at_each_heading",
        r##"
(let ((neomacs-org-superstar-test-hook-calls nil)
      (org-superstar-prettify-headline-hook
       '(neomacs-org-superstar-test-heading-hook)))
  (with-temp-buffer
    (neomacs-org-superstar-test-fontify "* A\n** B\n*** C\n")
    (list :calls (nreverse neomacs-org-superstar-test-hook-calls)
          :properties
          (save-excursion
            (goto-char (point-min))
            (let (result)
              (while (re-search-forward "^\\*+ " nil t)
                (push
                 (get-text-property (match-beginning 0)
                                    'neomacs-superstar-level)
                 result))
              (nreverse result))))))
"##,
        expect!["OK (:calls ((1 1) (2 2) (3 3)) :properties (1 2 3))"],
    )
}

fn configure_like_org_bullets_and_mode_lifecycle_restore_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "configure_like_org_bullets_and_mode_lifecycle_restore_rendering",
        r##"
(let ((org-hide-leading-stars nil)
      (org-superstar-cycle-headline-bullets nil)
      (org-superstar-special-todo-items t))
  (with-temp-buffer
    (insert "* Build\n*** Ship\n")
    (org-mode)
    (let ((configured
           (list :return (org-superstar-configure-like-org-bullets)
                 :hide-leading org-hide-leading-stars
                 :cycle org-superstar-cycle-headline-bullets
                 :todo org-superstar-special-todo-items)))
      (org-superstar-mode 1)
      (font-lock-ensure (point-min) (point-max))
      (let ((enabled (neomacs-org-superstar-test-headings)))
        (org-superstar-mode -1)
        (font-lock-ensure (point-min) (point-max))
        (let ((disabled (neomacs-org-superstar-test-headings)))
          (goto-char (point-max))
          (insert "** Verify\n")
          (org-superstar-mode 1)
          (font-lock-ensure (point-min) (point-max))
          (org-superstar-restart)
          (font-lock-ensure (point-min) (point-max))
          (list :configured configured
                :enabled enabled
                :disabled disabled
                :restarted (neomacs-org-superstar-test-headings)
                :mode org-superstar-mode
                :keyword-count
                (cl-count (car org-superstar--font-lock-keywords)
                          font-lock-keywords :test #'equal)))))))
"##,
        expect![[
            r#"OK (:configured (:return nil :hide-leading t :cycle t :todo nil) :enabled ((:raw "* Build" :level 1 :leading nil :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "*** Ship" :level 3 :leading ((:composition nil :face org-hide :invisible nil) (:composition nil :face org-hide :invisible nil)) :bullet (:composition "((1 . 10040))" :face (org-superstar-header-bullet org-level-3) :invisible nil))) :disabled ((:raw "* Build" :level 1 :leading nil :bullet (:composition nil :face org-level-1 :invisible nil)) (:raw "*** Ship" :level 3 :leading ((:composition nil :face org-hide :invisible nil) (:composition nil :face org-hide :invisible nil)) :bullet (:composition nil :face org-level-3 :invisible nil))) :restarted ((:raw "* Build" :level 1 :leading nil :bullet (:composition "((1 . 9673))" :face (org-superstar-header-bullet org-level-1) :invisible nil)) (:raw "*** Ship" :level 3 :leading ((:composition nil :face org-hide :invisible nil) (:composition nil :face org-hide :invisible nil)) :bullet (:composition "((1 . 10040))" :face (org-superstar-header-bullet org-level-3) :invisible nil)) (:raw "** Verify" :level 2 :leading ((:composition nil :face org-hide :invisible nil)) :bullet (:composition "((1 . 9675))" :face (org-superstar-header-bullet org-level-2) :invisible nil))) :mode t :keyword-count 1)"#
        ]],
    )
}

fn enabling_outside_org_mode_refuses_activation_without_global_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_outside_org_mode_refuses_activation_without_global_state",
        r##"
(with-temp-buffer
  (text-mode)
  (let ((message-log-max nil)
        message)
    (cl-letf (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (setq message (apply #'format format-string arguments)))))
      (org-superstar-mode 1))
    (list :mode org-superstar-mode
          :message message
          :pre-hook (and (memq #'org-superstar--invisibility-off
                               pre-command-hook)
                         t)
          :post-hook (and (memq #'org-superstar--invisibility-on
                                post-command-hook)
                          t)
          :invisibility
          (member '(org-superstar-hide) buffer-invisibility-spec))))
"##,
        expect![[
            r#"OK (:mode nil :message "Org mode is not enabled in this buffer." :pre-hook nil :post-hook nil :invisibility nil)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_headlines_cycle_bullets_and_prettify_leading_stars(),
        custom_cycle_rules_and_terminal_fallbacks_select_exact_bullets(),
        todo_bullets_support_exact_default_and_hidden_keyword_policies(),
        real_plain_lists_are_prettified_but_source_block_lookalikes_are_not(),
        lightweight_lists_intentionally_prettify_source_block_lookalikes(),
        headline_hook_can_add_user_visible_properties_at_each_heading(),
        configure_like_org_bullets_and_mode_lifecycle_restore_rendering(),
        enabling_outside_org_mode_refuses_activation_without_global_state(),
    ]
}
