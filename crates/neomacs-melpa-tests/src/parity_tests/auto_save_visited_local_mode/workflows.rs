use expect_test::expect;

use super::ParityBatchCase;

fn activation_uses_one_real_buffer_local_timer_and_cleans_up_on_disable() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-activation"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "release plan Ω.txt" root))
       buffer enabled disabled old-timer)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file path "release: draft\n")
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (setq-local auto-save-visited-local-interval 2.5)
          (setq-local auto-save-visited-local-silent nil)
          (let ((messages-start
                 (with-current-buffer (messages-buffer) (point-max))))
            (auto-save-visited-local-mode 1)
            (setq old-timer auto-save-visited-local--timer
                  enabled
                  (list
                   :messages
                   (neomacs-asvlm-test--messages messages-start)
                   :state (neomacs-asvlm-test--buffer-state buffer))))
          (let ((messages-start
                 (with-current-buffer (messages-buffer) (point-max))))
            (auto-save-visited-local-mode -1)
            (setq disabled
                  (list
                   :messages (neomacs-asvlm-test--messages messages-start)
                   :old-timer-registered
                   (and (memq old-timer timer-idle-list) t)
                   :state (neomacs-asvlm-test--buffer-state buffer))))))
    (neomacs-asvlm-test--cleanup (list buffer) root))
  (list :enabled enabled :disabled disabled))
"####;
    let expect = expect![[
        r#"OK (:enabled (:messages ("Auto-Save-Visited-Local mode enabled for release plan Ω.txt (interval: 2s)") :state (:live t :text "release: draft\n" :point 1 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 2.5 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))) :disabled (:messages ("Auto-Save-Visited-Local mode disabled for release plan Ω.txt") :old-timer-registered nil :state (:live t :text "release: draft\n" :point 1 :modified nil :read-only nil :mode nil :kill-hook nil :timer (:buffer-live t :present nil :idle-seconds nil :repeat nil :function nil :argument-is-buffer nil :registered nil))))"#
    ]];
    ParityBatchCase::value(
        "activation_uses_one_real_buffer_local_timer_and_cleans_up_on_disable",
        elisp_form,
        expect,
    )
}

fn independent_buffers_save_only_when_their_own_idle_timer_fires() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-independent"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (first-path (expand-file-name "project notes.txt" root))
       (second-path (expand-file-name "meeting notes.txt" root))
       first second sentinel timers-before after-first result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file first-path "project: draft\n")
        (neomacs-asvlm-test--write-file second-path "meeting: draft\n")
        (setq first (find-file-noselect first-path)
              second (find-file-noselect second-path)
              sentinel (generate-new-buffer " *auto-save-local-current*"))
        (setq neomacs-asvlm-test--events nil)
        (with-current-buffer first
          (setq-local make-backup-files nil)
          (setq-local auto-save-visited-local-interval 3)
          (setq-local auto-save-visited-local-silent t)
          (add-hook 'before-save-hook 'neomacs-asvlm-test--before-save nil t)
          (add-hook 'after-save-hook 'neomacs-asvlm-test--after-save nil t)
          (goto-char (point-max))
          (insert "project: ready Ω\n")
          (auto-save-visited-local-mode 1))
        (with-current-buffer second
          (setq-local make-backup-files nil)
          (setq-local auto-save-visited-local-interval 7)
          (setq-local auto-save-visited-local-silent t)
          (add-hook 'before-save-hook 'neomacs-asvlm-test--before-save nil t)
          (add-hook 'after-save-hook 'neomacs-asvlm-test--after-save nil t)
          (goto-char (point-max))
          (insert "meeting: approved λ\n")
          (auto-save-visited-local-mode 1))
        (setq timers-before
              (list
               :distinct
               (not
                (eq
                 (buffer-local-value
                  'auto-save-visited-local--timer first)
                 (buffer-local-value
                  'auto-save-visited-local--timer second)))
               :first (neomacs-asvlm-test--timer-state first)
               :second (neomacs-asvlm-test--timer-state second)))
        (set-buffer sentinel)
        (neomacs-asvlm-test--fire-buffer-timer first)
        (setq after-first
              (list
               :first-disk (neomacs-asvlm-test--file-text first-path)
               :first-buffer (neomacs-asvlm-test--buffer-state first)
               :current-buffer-preserved (eq (current-buffer) sentinel)
               :second-disk (neomacs-asvlm-test--file-text second-path)
               :second-buffer (neomacs-asvlm-test--buffer-state second)
               :save-events (nreverse neomacs-asvlm-test--events)))
        (setq neomacs-asvlm-test--events nil)
        (neomacs-asvlm-test--fire-buffer-timer second)
        (setq result
              (list
               :timers-before timers-before
               :after-first-idle after-first
               :after-second-idle
               (list
                :first-disk (neomacs-asvlm-test--file-text first-path)
                :first-buffer (neomacs-asvlm-test--buffer-state first)
                :current-buffer-preserved (eq (current-buffer) sentinel)
                :second-disk (neomacs-asvlm-test--file-text second-path)
                :second-buffer (neomacs-asvlm-test--buffer-state second)
                :save-events (nreverse neomacs-asvlm-test--events)))))
    (neomacs-asvlm-test--cleanup (list first second sentinel) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:timers-before (:distinct t :first (:buffer-live t :present t :idle-seconds 3.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t) :second (:buffer-live t :present t :idle-seconds 7.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :after-first-idle (:first-disk "project: draft\nproject: ready Ω\n" :first-buffer (:live t :text "project: draft\nproject: ready Ω\n" :point 33 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 3.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :current-buffer-preserved t :second-disk "meeting: draft\n" :second-buffer (:live t :text "meeting: draft\nmeeting: approved λ\n" :point 36 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 7.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :save-events ((:before "project notes.txt" t nil) (:after "project notes.txt" t nil))) :after-second-idle (:first-disk "project: draft\nproject: ready Ω\n" :first-buffer (:live t :text "project: draft\nproject: ready Ω\n" :point 33 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 3.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :current-buffer-preserved t :second-disk "meeting: draft\nmeeting: approved λ\n" :second-buffer (:live t :text "meeting: draft\nmeeting: approved λ\n" :point 36 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 7.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :save-events ((:before "meeting notes.txt" t nil) (:after "meeting notes.txt" t nil))))"#
    ]];
    ParityBatchCase::value(
        "independent_buffers_save_only_when_their_own_idle_timer_fires",
        elisp_form,
        expect,
    )
}

fn interval_watcher_restarts_the_timer_with_the_previous_local_value() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
       (file-name-as-directory
         (expand-file-name "auto-save-local-watcher"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "watcher.txt" root))
       buffer old-default first second third result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file path "watcher\n")
        (setq buffer (find-file-noselect path))
        (with-temp-buffer
          (auto-save-visited-local-mode-turn-on))
        (setq old-default auto-save-visited-local-interval)
        (with-current-buffer buffer
          (setq-local auto-save-visited-local-silent t)
          (setq-local auto-save-visited-local-interval 4)
          (auto-save-visited-local-mode 1)
          (setq first auto-save-visited-local--timer)
          (let ((initial (neomacs-asvlm-test--timer-state buffer)))
            (setq-local auto-save-visited-local-interval 9)
            (setq second auto-save-visited-local--timer)
            (let ((after-first-change
                   (list
                    :configured auto-save-visited-local-interval
                    :timer-replaced (not (eq first second))
                    :old-registered (and (memq first timer-idle-list) t)
                    :timer (neomacs-asvlm-test--timer-state buffer))))
              (setq-local auto-save-visited-local-interval 12)
              (setq third auto-save-visited-local--timer)
              (let ((after-second-change
                     (list
                      :configured auto-save-visited-local-interval
                      :timer-replaced (not (eq second third))
                      :old-registered (and (memq second timer-idle-list) t)
                      :timer (neomacs-asvlm-test--timer-state buffer))))
                (set-default 'auto-save-visited-local-interval 31)
                (setq result
                      (list
                       :initial initial
                       :after-first-local-change after-first-change
                       :after-second-local-change after-second-change
                       :after-default-change
                       (list
                        :default (default-value
                                  'auto-save-visited-local-interval)
                        :local auto-save-visited-local-interval
                        :same-timer (eq third auto-save-visited-local--timer)
                        :timer (neomacs-asvlm-test--timer-state buffer)))))))))
    (when old-default
      (set-default 'auto-save-visited-local-interval old-default))
    (neomacs-asvlm-test--cleanup (list buffer) root))
  result)
"####;
    let expect = expect![
        "OK (:initial (:buffer-live t :present t :idle-seconds 4.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t) :after-first-local-change (:configured 9 :timer-replaced t :old-registered nil :timer (:buffer-live t :present t :idle-seconds 4.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :after-second-local-change (:configured 12 :timer-replaced t :old-registered nil :timer (:buffer-live t :present t :idle-seconds 9.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :after-default-change (:default 31 :local 12 :same-timer t :timer (:buffer-live t :present t :idle-seconds 9.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)))"
    ];
    ParityBatchCase::value(
        "interval_watcher_restarts_the_timer_with_the_previous_local_value",
        elisp_form,
        expect,
    )
}

fn predicate_blocks_drafts_then_saves_the_ready_document() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-predicate"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "deployment.txt" root))
       buffer blocked result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file path "status: DRAFT\nowner: team\n")
        (setq buffer (find-file-noselect path)
              neomacs-asvlm-test--predicate-calls 0)
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (setq-local auto-save-visited-local-silent t)
          (setq-local auto-save-visited-local-predicate
                      'neomacs-asvlm-test--ready-document-p)
          (goto-char (point-max))
          (insert "approval: pending Ω\n")
          (auto-save-visited-local-mode 1)
          (neomacs-asvlm-test--fire-buffer-timer buffer)
          (setq blocked
                (list
                 :predicate-calls neomacs-asvlm-test--predicate-calls
                 :disk (neomacs-asvlm-test--file-text path)
                 :buffer (neomacs-asvlm-test--buffer-state buffer)))
          (goto-char (point-min))
          (search-forward "DRAFT")
          (replace-match "READY" t t)
          (neomacs-asvlm-test--fire-buffer-timer buffer)
          (setq result
                (list
                 :blocked-draft blocked
                 :saved-ready
                 (list
                  :predicate-calls neomacs-asvlm-test--predicate-calls
                  :disk (neomacs-asvlm-test--file-text path)
                  :buffer (neomacs-asvlm-test--buffer-state buffer))))))
    (neomacs-asvlm-test--cleanup (list buffer) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:blocked-draft (:predicate-calls 1 :disk "status: DRAFT\nowner: team\n" :buffer (:live t :text "status: DRAFT\nowner: team\napproval: pending Ω\n" :point 47 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))) :saved-ready (:predicate-calls 2 :disk "status: READY\nowner: team\napproval: pending Ω\n" :buffer (:live t :text "status: READY\nowner: team\napproval: pending Ω\n" :point 14 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))))"#
    ]];
    ParityBatchCase::value(
        "predicate_blocks_drafts_then_saves_the_ready_document",
        elisp_form,
        expect,
    )
}

fn visible_and_silent_idle_saves_run_real_save_hooks_in_their_documented_environment()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-save-hooks"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (visible-path (expand-file-name "visible notes.txt" root))
       (silent-path (expand-file-name "silent notes.txt" root))
       visible silent messages-start result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file visible-path "visible: draft")
        (neomacs-asvlm-test--write-file silent-path "silent: draft")
        (setq visible (find-file-noselect visible-path)
              silent (find-file-noselect silent-path))
        (dolist (buffer (list visible silent))
          (with-current-buffer buffer
            (setq-local make-backup-files nil)
            (setq-local require-final-newline t)
            (add-hook 'before-save-hook 'neomacs-asvlm-test--before-save nil t)
            (add-hook 'after-save-hook 'neomacs-asvlm-test--after-save nil t)))
        (with-current-buffer visible
          (setq-local auto-save-visited-local-silent nil)
          (goto-char (point-max))
          (insert "\nvisible: ready Ω")
          (auto-save-visited-local-mode 1))
        (with-current-buffer silent
          (setq-local auto-save-visited-local-silent t)
          (goto-char (point-max))
          (insert "\nsilent: ready λ")
          (auto-save-visited-local-mode 1))
        (setq neomacs-asvlm-test--events nil
              messages-start (with-current-buffer (messages-buffer) (point-max)))
        (neomacs-asvlm-test--fire-buffer-timer visible)
        (neomacs-asvlm-test--fire-buffer-timer silent)
        (setq result
              (list
               :messages (neomacs-asvlm-test--messages messages-start)
               :save-events (nreverse neomacs-asvlm-test--events)
               :visible
               (list
                :disk (neomacs-asvlm-test--file-text visible-path)
                :buffer (neomacs-asvlm-test--buffer-state visible))
               :silent
               (list
                :disk (neomacs-asvlm-test--file-text silent-path)
                :buffer (neomacs-asvlm-test--buffer-state silent)))))
    (neomacs-asvlm-test--cleanup (list visible silent) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:messages nil :save-events ((:before "visible notes.txt" nil nil) (:after "visible notes.txt" nil nil) (:before "silent notes.txt" t nil) (:after "silent notes.txt" t nil)) :visible (:disk "visible: draft\nvisible: ready Ω\n" :buffer (:live t :text "visible: draft\nvisible: ready Ω\n" :point 32 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))) :silent (:disk "silent: draft\nsilent: ready λ\n" :buffer (:live t :text "silent: draft\nsilent: ready λ\n" :point 30 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))))"#
    ]];
    ParityBatchCase::value(
        "visible_and_silent_idle_saves_run_real_save_hooks_in_their_documented_environment",
        elisp_form,
        expect,
    )
}

fn save_integration_errors_are_reported_or_suppressed_without_stopping_the_timer() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-errors"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (visible-path (expand-file-name "visible failure.txt" root))
       (silent-path (expand-file-name "silent failure.txt" root))
       visible silent messages-start result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file visible-path "visible: old\n")
        (neomacs-asvlm-test--write-file silent-path "silent: old\n")
        (setq visible (find-file-noselect visible-path)
              silent (find-file-noselect silent-path))
        (dolist (buffer (list visible silent))
          (with-current-buffer buffer
            (setq-local make-backup-files nil)
            (setq-local write-contents-functions
                        '(neomacs-asvlm-test--reject-save))
            (add-hook 'before-save-hook 'neomacs-asvlm-test--before-save nil t)
            (add-hook 'after-save-hook 'neomacs-asvlm-test--after-save nil t)
            (goto-char (point-max))
            (insert "unsaved edit Ω\n")))
        (with-current-buffer visible
          (setq-local auto-save-visited-local-silent nil)
          (auto-save-visited-local-mode 1))
        (with-current-buffer silent
          (setq-local auto-save-visited-local-silent t)
          (auto-save-visited-local-mode 1))
        (setq neomacs-asvlm-test--events nil
              messages-start (with-current-buffer (messages-buffer) (point-max)))
        (neomacs-asvlm-test--fire-buffer-timer visible)
        (neomacs-asvlm-test--fire-buffer-timer silent)
        (setq result
              (list
               :messages (neomacs-asvlm-test--messages messages-start)
               :save-events (nreverse neomacs-asvlm-test--events)
               :visible
               (list
                :disk (neomacs-asvlm-test--file-text visible-path)
                :buffer (neomacs-asvlm-test--buffer-state visible))
               :silent
               (list
                :disk (neomacs-asvlm-test--file-text silent-path)
                :buffer (neomacs-asvlm-test--buffer-state silent)))))
    (neomacs-asvlm-test--cleanup (list visible silent) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:messages ("Auto-save-visited-local error: formatter rejected save Ω") :save-events ((:before "visible failure.txt" nil nil) (:before "silent failure.txt" t nil)) :visible (:disk "visible: old\n" :buffer (:live t :text "visible: old\nunsaved edit Ω\n" :point 29 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))) :silent (:disk "silent: old\n" :buffer (:live t :text "silent: old\nunsaved edit Ω\n" :point 28 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))))"#
    ]];
    ParityBatchCase::value(
        "save_integration_errors_are_reported_or_suppressed_without_stopping_the_timer",
        elisp_form,
        expect,
    )
}

fn turn_on_helper_from_text_mode_hook_and_direct_kill_cancel_the_registered_timer()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-direct-kill"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "hooked document.txt" root))
       buffer timer before-kill after-kill)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file path "hooked: draft\n")
        (with-temp-buffer
          (auto-save-visited-local-mode-turn-on))
        (let ((text-mode-hook '(auto-save-visited-local-mode-turn-on))
              (auto-save-visited-local-interval 6)
              (auto-save-visited-local-silent t))
          (setq buffer (find-file-noselect path)
                timer
                (buffer-local-value
                 'auto-save-visited-local--timer buffer)
                before-kill (neomacs-asvlm-test--buffer-state buffer))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)
          (setq after-kill
                (list
                 :buffer-live (and (buffer-live-p buffer) t)
                 :timer-registered (and (memq timer timer-idle-list) t)))))
    (neomacs-asvlm-test--cleanup (list buffer) root))
  (list :before-kill before-kill :after-kill after-kill))
"####;
    let expect = expect![[
        r#"OK (:before-kill (:live t :text "hooked: draft\n" :point 1 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 6.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :after-kill (:buffer-live nil :timer-registered nil))"#
    ]];
    ParityBatchCase::value(
        "turn_on_helper_from_text_mode_hook_and_direct_kill_cancel_the_registered_timer",
        elisp_form,
        expect,
    )
}

fn cloned_indirect_buffer_inherits_and_cancels_the_base_buffers_timer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-indirect-clone"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (guard-path (expand-file-name "guard document.txt" root))
       (base-path (expand-file-name "active document.txt" root))
       guard-base guard-clone base clone timer guard-state before-clone-kill result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file guard-path "guard: unchanged\n")
        (neomacs-asvlm-test--write-file base-path "active: old\n")
        (with-temp-buffer
          (auto-save-visited-local-mode-turn-on))
        (let ((text-mode-hook nil))
          (setq guard-base (find-file-noselect guard-path)))
        (setq guard-clone
              (with-current-buffer guard-base
                (clone-indirect-buffer "guarded indirect" nil)))
        (with-current-buffer guard-clone
          (auto-save-visited-local-mode-turn-on))
        (setq guard-state (neomacs-asvlm-test--buffer-state guard-clone))
        (neomacs-asvlm-test--cleanup (list guard-clone) nil)
        (let ((text-mode-hook '(auto-save-visited-local-mode-turn-on))
              (auto-save-visited-local-interval 6)
              (auto-save-visited-local-silent t))
          (setq base (find-file-noselect base-path))
          (with-current-buffer base
            (setq-local make-backup-files nil)
            (goto-char (point-max))
            (insert "active: pending Ω\n")
            (setq clone
                  (clone-indirect-buffer "active indirect" nil)))
          (setq timer
                (buffer-local-value
                 'auto-save-visited-local--timer base)
                before-clone-kill
                (list
                 :shared-timer
                 (eq
                  timer
                  (buffer-local-value
                   'auto-save-visited-local--timer clone))
                 :base (neomacs-asvlm-test--buffer-state base)
                 :clone (neomacs-asvlm-test--buffer-state clone)))
          (kill-buffer clone)
          (neomacs-asvlm-test--fire-buffer-timer base)
          (setq result
              (list
               :guarded-indirect guard-state
               :before-clone-kill before-clone-kill
               :after-clone-kill-and-idle
               (list
                :clone-live (and (buffer-live-p clone) t)
                :timer-registered (and (memq timer timer-idle-list) t)
                :disk (neomacs-asvlm-test--file-text base-path)
                :base (neomacs-asvlm-test--buffer-state base))))))
    (neomacs-asvlm-test--cleanup
     (list guard-clone guard-base clone base) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:guarded-indirect (:live t :text "guard: unchanged\n" :point 1 :modified nil :read-only nil :mode nil :kill-hook nil :timer (:buffer-live t :present nil :idle-seconds nil :repeat nil :function nil :argument-is-buffer nil :registered nil)) :before-clone-kill (:shared-timer t :base (:live t :text "active: old\nactive: pending Ω\n" :point 31 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 6.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)) :clone (:live t :text "active: old\nactive: pending Ω\n" :point 31 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 6.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer nil :registered t))) :after-clone-kill-and-idle (:clone-live nil :timer-registered nil :disk "active: old\n" :base (:live t :text "active: old\nactive: pending Ω\n" :point 31 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 6.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered nil))))"#
    ]];
    ParityBatchCase::value(
        "cloned_indirect_buffer_inherits_and_cancels_the_base_buffers_timer",
        elisp_form,
        expect,
    )
}

fn enabling_in_a_non_file_buffer_signals_and_rolls_back_the_mode() -> ParityBatchCase {
    let elisp_form = r####"
(let (buffer outcome result)
  (unwind-protect
      (progn
        (setq buffer (generate-new-buffer " *auto-save-local-non-file*"))
        (with-current-buffer buffer
          (setq outcome
                (condition-case error-data
                    (progn
                      (auto-save-visited-local-mode 1)
                      :unexpected-success)
                  (error
                   (list (car error-data) (cdr error-data)))))
          (setq result
                (list
                 :outcome outcome
                 :registered-despite-error
                 (and
                  (memq 'auto-save-visited-local-mode local-minor-modes)
                  t)
                 :state (neomacs-asvlm-test--buffer-state buffer)
                 :turn-on-helper-result
                 (auto-save-visited-local-mode-turn-on)
                 :state-after-helper
                 (neomacs-asvlm-test--buffer-state buffer)))))
    (neomacs-asvlm-test--cleanup (list buffer) nil))
  result)
"####;
    let expect = expect![[
        r#"OK (:outcome (user-error ("Buffer must be visiting a file to use auto-save-visited-local")) :registered-despite-error t :state (:live t :text "" :point 1 :modified nil :read-only nil :mode nil :kill-hook nil :timer (:buffer-live t :present nil :idle-seconds nil :repeat nil :function nil :argument-is-buffer nil :registered nil)) :turn-on-helper-result nil :state-after-helper (:live t :text "" :point 1 :modified nil :read-only nil :mode nil :kill-hook nil :timer (:buffer-live t :present nil :idle-seconds nil :repeat nil :function nil :argument-is-buffer nil :registered nil)))"#
    ]];
    ParityBatchCase::value(
        "enabling_in_a_non_file_buffer_signals_and_rolls_back_the_mode",
        elisp_form,
        expect,
    )
}

fn remote_file_handler_checks_writability_before_the_remote_file_rejection() -> ParityBatchCase {
    let elisp_form = r####"
(let ((remote-name "/neomacs-asvlm-remote:server:/work/notes Ω.txt")
      buffer result)
  (unwind-protect
      (progn
        (setq buffer (generate-new-buffer "remote work notes Ω.txt"))
        (let ((file-name-handler-alist
               '(("\\`/neomacs-asvlm-remote:"
                  . neomacs-asvlm-test--remote-file-handler))))
          (with-current-buffer buffer
            (setq buffer-file-name remote-name)
            (setq-local auto-save-visited-local-silent t)
            (insert "remote edit remains local λ\n")
            (setq neomacs-asvlm-test--file-handler-events nil)
            (auto-save-visited-local-mode 1)
            (neomacs-asvlm-test--fire-buffer-timer buffer)
            (setq result
                  (list
                   :file-handler-operations
                   (nreverse neomacs-asvlm-test--file-handler-events)
                   :buffer (neomacs-asvlm-test--buffer-state buffer))))))
    (neomacs-asvlm-test--cleanup (list buffer) nil))
  result)
"####;
    let expect = expect![[
        r#"OK (:file-handler-operations ((expand-file-name "/neomacs-asvlm-remote:server:/work/notes Ω.txt" nil) (file-writable-p "/neomacs-asvlm-remote:server:/work/notes Ω.txt") (file-remote-p "/neomacs-asvlm-remote:server:/work/notes Ω.txt" nil nil)) :buffer (:live t :text "remote edit remains local λ\n" :point 29 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)))"#
    ]];
    ParityBatchCase::value(
        "remote_file_handler_checks_writability_before_the_remote_file_rejection",
        elisp_form,
        expect,
    )
}

fn a_deleted_file_is_recreated_while_an_unwritable_file_remains_dirty() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-local-eligibility"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (deleted-path (expand-file-name "deleted notes.txt" root))
       (locked-path (expand-file-name "protected notes.txt" root))
       deleted locked after-first-idle result)
  (unwind-protect
      (progn
        (neomacs-asvlm-test--write-file deleted-path "deleted: old\n")
        (neomacs-asvlm-test--write-file locked-path "protected: old\n")
        (setq deleted (find-file-noselect deleted-path)
              locked (find-file-noselect locked-path))
        (dolist (buffer (list deleted locked))
          (with-current-buffer buffer
            (setq-local make-backup-files nil)
            (setq-local auto-save-visited-local-silent t)
            (goto-char (point-max))
            (insert
             (if (eq buffer deleted)
                 "deleted: recovered Ω\n"
               "protected: local edit λ\n"))
            (auto-save-visited-local-mode 1)))
        (delete-file deleted-path)
        (set-file-modes locked-path #o444)
        (let ((preconditions
              (list
               :deleted-exists (and (file-exists-p deleted-path) t)
               :deleted-writable (and (file-writable-p deleted-path) t)
               :locked-writable (and (file-writable-p locked-path) t))))
          (neomacs-asvlm-test--fire-buffer-timer deleted)
          (neomacs-asvlm-test--fire-buffer-timer locked)
          (setq after-first-idle
                (list
                 :preconditions preconditions
                 :deleted
                 (list
                  :exists (and (file-exists-p deleted-path) t)
                  :disk (neomacs-asvlm-test--file-text deleted-path)
                  :buffer (neomacs-asvlm-test--buffer-state deleted))
                 :locked
                 (list
                  :disk (neomacs-asvlm-test--file-text locked-path)
                  :buffer (neomacs-asvlm-test--buffer-state locked)))))
        (set-file-modes locked-path #o644)
        (neomacs-asvlm-test--fire-buffer-timer locked)
        (setq result
              (list
               :after-first-idle after-first-idle
               :locked-after-permission-restored
               (list
                :disk (neomacs-asvlm-test--file-text locked-path)
                :buffer (neomacs-asvlm-test--buffer-state locked)))))
    (when (file-exists-p locked-path)
      (set-file-modes locked-path #o644))
    (neomacs-asvlm-test--cleanup (list deleted locked) root))
  result)
"####;
    let expect = expect![[
        r#"OK (:after-first-idle (:preconditions (:deleted-exists nil :deleted-writable t :locked-writable nil) :deleted (:exists t :disk "deleted: old\ndeleted: recovered Ω\n" :buffer (:live t :text "deleted: old\ndeleted: recovered Ω\n" :point 35 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))) :locked (:disk "protected: old\n" :buffer (:live t :text "protected: old\nprotected: local edit λ\n" :point 40 :modified t :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t)))) :locked-after-permission-restored (:disk "protected: old\nprotected: local edit λ\n" :buffer (:live t :text "protected: old\nprotected: local edit λ\n" :point 40 :modified nil :read-only nil :mode t :kill-hook t :timer (:buffer-live t :present t :idle-seconds 5.0 :repeat t :function auto-save-visited-local--save-buffer-wrapper :argument-is-buffer t :registered t))))"#
    ]];
    ParityBatchCase::value(
        "a_deleted_file_is_recreated_while_an_unwritable_file_remains_dirty",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        activation_uses_one_real_buffer_local_timer_and_cleans_up_on_disable(),
        independent_buffers_save_only_when_their_own_idle_timer_fires(),
        interval_watcher_restarts_the_timer_with_the_previous_local_value(),
        predicate_blocks_drafts_then_saves_the_ready_document(),
        visible_and_silent_idle_saves_run_real_save_hooks_in_their_documented_environment(),
        save_integration_errors_are_reported_or_suppressed_without_stopping_the_timer(),
        turn_on_helper_from_text_mode_hook_and_direct_kill_cancel_the_registered_timer(),
        cloned_indirect_buffer_inherits_and_cancels_the_base_buffers_timer(),
        enabling_in_a_non_file_buffer_signals_and_rolls_back_the_mode(),
        remote_file_handler_checks_writability_before_the_remote_file_rejection(),
        a_deleted_file_is_recreated_while_an_unwritable_file_remains_dirty(),
    ]
}
