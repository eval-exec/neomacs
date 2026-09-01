use expect_test::expect;

use super::ParityBatchCase;

fn activation_schedules_real_repeating_idle_timers_and_optional_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((after-init-hook nil)
       (elscreen-was-bound (boundp 'elscreen-create-hook))
       (elscreen-old-value
        (and elscreen-was-bound (symbol-value 'elscreen-create-hook)))
       before after-load after-nil after-first after-second result)
  (setq elscreen-create-hook nil)
  (neomacs-asbe-test--cancel-package-timers)
  (unwind-protect
      (progn
        (setq before
              (list
               :feature (featurep 'auto-save-buffers-enhanced)
               :public-entries
               (mapcar
                (lambda (entry)
                  (list
                   entry
                   :autoload
                   (and (autoloadp (symbol-function entry)) t)
                   :command (and (commandp entry) t)))
                '(auto-save-buffers-enhanced
                  auto-save-buffers-enhanced-include-only-checkout-path
                  auto-save-buffers-enhanced-toggle-activity
                  auto-save-buffers-enhanced-reload-svk))
               :internal-savers-bound
               (mapcar
                #'fboundp
                '(auto-save-buffers-enhanced-save-buffers
                  auto-save-buffers-enhanced-saver-buffer
                  auto-save-buffers-enhanced-quiet-save-buffer))))
        (auto-save-buffers-enhanced nil)
        (setq after-load
              (list
               :feature (featurep 'auto-save-buffers-enhanced)
               :public-entry-autoloads
               (mapcar
                (lambda (entry)
                  (and (autoloadp (symbol-function entry)) t))
                '(auto-save-buffers-enhanced
                  auto-save-buffers-enhanced-include-only-checkout-path
                  auto-save-buffers-enhanced-toggle-activity
                  auto-save-buffers-enhanced-reload-svk))
               :internal-savers-bound
               (mapcar
                #'fboundp
                '(auto-save-buffers-enhanced-save-buffers
                  auto-save-buffers-enhanced-saver-buffer
                  auto-save-buffers-enhanced-quiet-save-buffer))))
        (let ((auto-save-buffers-enhanced-interval 2.5)
              (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p t)
              (auto-save-buffers-enhanced-cooperate-elscreen-p t))
          (auto-save-buffers-enhanced nil)
          (setq after-nil
                (list
                 :timer (neomacs-asbe-test--timer-state)
                 :scratch-hook
                 (neomacs-asbe-test--hook-count
                  'auto-save-buffers-enhanced-scratch-read-after-init-hook
                  after-init-hook)
                 :elscreen-hook
                 (neomacs-asbe-test--hook-count
                  'auto-save-buffers-enhanced-cooperate-elscreen-default-window
                  (symbol-value 'elscreen-create-hook))))
          (auto-save-buffers-enhanced t)
          (setq after-first (neomacs-asbe-test--timer-state))
          (auto-save-buffers-enhanced t)
          (setq after-second (neomacs-asbe-test--timer-state))
          (setq result
                (list
                 :before before
                 :after-load after-load
                 :nil-flag after-nil
                 :first-enable after-first
                 :second-enable after-second
                 :hooks-after-repeat
                 (list
                  :scratch
                  (neomacs-asbe-test--hook-count
                   'auto-save-buffers-enhanced-scratch-read-after-init-hook
                   after-init-hook)
                  :elscreen
                  (neomacs-asbe-test--hook-count
                   'auto-save-buffers-enhanced-cooperate-elscreen-default-window
                   (symbol-value 'elscreen-create-hook)))))))
    (neomacs-asbe-test--cancel-package-timers)
    (if elscreen-was-bound
        (set 'elscreen-create-hook elscreen-old-value)
      (makunbound 'elscreen-create-hook)))
  result)
"####;
    let expect = expect![
        "OK (:before (:feature nil :public-entries ((auto-save-buffers-enhanced :autoload t :command nil) (auto-save-buffers-enhanced-include-only-checkout-path :autoload t :command nil) (auto-save-buffers-enhanced-toggle-activity :autoload t :command t) (auto-save-buffers-enhanced-reload-svk :autoload t :command t)) :internal-savers-bound (nil nil nil)) :after-load (:feature t :public-entry-autoloads (nil nil nil nil) :internal-savers-bound (t t t)) :nil-flag (:timer (:count 0 :timers nil) :scratch-hook 1 :elscreen-hook 1) :first-enable (:count 1 :timers ((:idle-seconds 2.5 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t))) :second-enable (:count 2 :timers ((:idle-seconds 2.5 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t) (:idle-seconds 2.5 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t))) :hooks-after-repeat (:scratch 1 :elscreen 1))"
    ];
    ParityBatchCase::value(
        "activation_schedules_real_repeating_idle_timers_and_optional_hooks",
        elisp_form,
        expect,
    )
}

fn idle_tick_saves_only_included_writable_editable_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-filtering"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (included-path (expand-file-name "notes Ω.txt" root))
       (excluded-path (expand-file-name "draft.ignore" root))
       (read-only-path (expand-file-name "locked.txt" root))
       (unwritable-path (expand-file-name "permissions.txt" root))
       (unchanged-path (expand-file-name "unchanged.txt" root))
       (sentinel (generate-new-buffer " *auto-save-current*"))
       included excluded read-only unwritable unchanged result)
  (neomacs-asbe-test--write-file included-path "included: old\n")
  (neomacs-asbe-test--write-file excluded-path "excluded: old\n")
  (neomacs-asbe-test--write-file read-only-path "locked: old\n")
  (neomacs-asbe-test--write-file unwritable-path "permissions: old\n")
  (neomacs-asbe-test--write-file unchanged-path "unchanged\n")
  (setq included (find-file-noselect included-path)
        excluded (find-file-noselect excluded-path)
        read-only (find-file-noselect read-only-path)
        unwritable (find-file-noselect unwritable-path)
        unchanged (find-file-noselect unchanged-path))
  (neomacs-asbe-test--load-package)
  (setq neomacs-asbe-test--events nil)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps
             (list (concat "^" (regexp-quote root))))
            (auto-save-buffers-enhanced-exclude-regexps '("\\.ignore\\'"))
            (auto-save-buffers-enhanced-activity-flag t)
            (auto-save-buffers-enhanced-quiet-save-p nil)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p nil)
            (auto-save-buffers-enhanced-cooperate-elscreen-p nil))
        (dolist (buffer (list included excluded read-only unwritable unchanged))
          (with-current-buffer buffer
            (setq-local make-backup-files nil)))
        (with-current-buffer included
          (add-hook 'before-save-hook
                    (lambda ()
                      (setq neomacs-asbe-test--events
                            (cons :before neomacs-asbe-test--events)))
                    nil t)
          (add-hook 'after-save-hook
                    (lambda ()
                      (setq neomacs-asbe-test--events
                            (cons :after neomacs-asbe-test--events)))
                    nil t)
          (goto-char (point-max))
          (insert "included: new Ω\n"))
        (with-current-buffer excluded
          (goto-char (point-max))
          (insert "excluded: new λ\n"))
        (with-current-buffer read-only
          (goto-char (point-max))
          (insert "locked: local edit\n")
          (setq buffer-read-only t))
        (with-current-buffer unwritable
          (goto-char (point-max))
          (insert "permissions: local edit\n"))
        (set-file-modes unwritable-path #o444)
        (set-buffer sentinel)
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (let ((timer-before (neomacs-asbe-test--timer-state)))
          (neomacs-asbe-test--fire-idle-tick)
          (setq result
                (list
                 :timer-before timer-before
                 :timer-after (neomacs-asbe-test--timer-state)
                 :current-buffer-preserved (eq (current-buffer) sentinel)
                 :save-events (nreverse neomacs-asbe-test--events)
                 :included
                 (list
                  :disk (neomacs-asbe-test--file-text included-path)
                  :buffer (neomacs-asbe-test--buffer-state included))
                 :excluded
                 (list
                  :disk (neomacs-asbe-test--file-text excluded-path)
                  :buffer (neomacs-asbe-test--buffer-state excluded))
                 :read-only
                 (list
                  :disk (neomacs-asbe-test--file-text read-only-path)
                  :buffer (neomacs-asbe-test--buffer-state read-only))
                 :unwritable
                 (list
                  :file-writable (and (file-writable-p unwritable-path) t)
                  :disk (neomacs-asbe-test--file-text unwritable-path)
                  :buffer (neomacs-asbe-test--buffer-state unwritable))
                 :unchanged
                 (list
                  :disk (neomacs-asbe-test--file-text unchanged-path)
                  :buffer (neomacs-asbe-test--buffer-state unchanged))))))
    (neomacs-asbe-test--cancel-package-timers)
    (setq neomacs-asbe-test--events nil)
    (when (file-exists-p unwritable-path)
      (set-file-modes unwritable-path #o644))
    (neomacs-asbe-test--cleanup-buffers
     (list included excluded read-only unwritable unchanged sentinel))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:timer-before (:count 1 :timers ((:idle-seconds 60.0 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t))) :timer-after (:count 1 :timers ((:idle-seconds 60.0 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t))) :current-buffer-preserved t :save-events (:before :after) :included (:disk "included: old\nincluded: new Ω\n" :buffer (:text "included: old\nincluded: new Ω\n" :point 31 :modified nil :read-only nil)) :excluded (:disk "excluded: old\n" :buffer (:text "excluded: old\nexcluded: new λ\n" :point 31 :modified t :read-only nil)) :read-only (:disk "locked: old\n" :buffer (:text "locked: old\nlocked: local edit\n" :point 32 :modified t :read-only t)) :unwritable (:file-writable nil :disk "permissions: old\n" :buffer (:text "permissions: old\npermissions: local edit\n" :point 42 :modified t :read-only nil)) :unchanged (:disk "unchanged\n" :buffer (:text "unchanged\n" :point 1 :modified nil :read-only nil)))"#
    ]];
    ParityBatchCase::value(
        "idle_tick_saves_only_included_writable_editable_buffers",
        elisp_form,
        expect,
    )
}

fn interactive_activity_toggle_pauses_and_resumes_the_same_timer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-activity"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "journal.txt" root))
       buffer off-message off-messages paused on-message on-messages resumed)
  (neomacs-asbe-test--write-file path "entry: old\n")
  (setq buffer (find-file-noselect path))
  (neomacs-asbe-test--load-package)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps '(".+"))
            (auto-save-buffers-enhanced-exclude-regexps nil)
            (auto-save-buffers-enhanced-activity-flag t)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p nil)
            (auto-save-buffers-enhanced-cooperate-elscreen-p nil))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (goto-char (point-max))
          (insert "entry: pending Ω\n"))
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (let ((start (with-current-buffer (messages-buffer) (point-max))))
          (call-interactively 'auto-save-buffers-enhanced-toggle-activity)
          (setq off-message (current-message)
                off-messages (neomacs-asbe-test--messages start)))
        (neomacs-asbe-test--fire-idle-tick)
        (setq paused
              (list
               :activity auto-save-buffers-enhanced-activity-flag
               :message off-message
               :messages off-messages
               :disk (neomacs-asbe-test--file-text path)
               :buffer (neomacs-asbe-test--buffer-state buffer)
               :timer (neomacs-asbe-test--timer-state)))
        (let ((start (with-current-buffer (messages-buffer) (point-max))))
          (call-interactively 'auto-save-buffers-enhanced-toggle-activity)
          (setq on-message (current-message)
                on-messages (neomacs-asbe-test--messages start)))
        (neomacs-asbe-test--fire-idle-tick)
        (setq resumed
              (list
               :activity auto-save-buffers-enhanced-activity-flag
               :message on-message
               :messages on-messages
               :disk (neomacs-asbe-test--file-text path)
               :buffer (neomacs-asbe-test--buffer-state buffer)
               :timer (neomacs-asbe-test--timer-state))))
    (neomacs-asbe-test--cancel-package-timers)
    (neomacs-asbe-test--cleanup-buffers (list buffer))
    (neomacs-asbe-test--cleanup-root root))
  (list :paused paused :resumed resumed))
"####;
    let expect = expect![[
        r#"OK (:paused (:activity nil :message nil :messages ("auto-save-buffers-enhanced off") :disk "entry: old\n" :buffer (:text "entry: old\nentry: pending Ω\n" :point 29 :modified t :read-only nil) :timer (:count 1 :timers ((:idle-seconds 60.0 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t)))) :resumed (:activity t :message nil :messages ("auto-save-buffers-enhanced on") :disk "entry: old\nentry: pending Ω\n" :buffer (:text "entry: old\nentry: pending Ω\n" :point 29 :modified nil :read-only nil) :timer (:count 1 :timers ((:idle-seconds 60.0 :repeat t :function auto-save-buffers-enhanced-save-buffers :arguments nil :registered t)))))"#
    ]];
    ParityBatchCase::value(
        "interactive_activity_toggle_pauses_and_resumes_the_same_timer",
        elisp_form,
        expect,
    )
}

fn quiet_save_persists_bytes_runs_hooks_and_clears_visited_modtime() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-quiet"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "release notes λ.txt" root))
       (original-message-function (symbol-function 'message))
       (original-write-function (symbol-function 'write-region))
       (alias-was-bound (fboundp 'original-write-region))
       (alias-old-function
        (and alias-was-bound (symbol-function 'original-write-region)))
       messages-start buffer first-save result)
  (neomacs-asbe-test--write-file path "release: draft\n")
  (setq buffer (find-file-noselect path))
  (setq neomacs-asbe-test--events nil)
  (neomacs-asbe-test--load-package)
  (setq messages-start (with-current-buffer (messages-buffer) (point-max)))
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps '(".+"))
            (auto-save-buffers-enhanced-exclude-regexps nil)
            (auto-save-buffers-enhanced-activity-flag t)
            (auto-save-buffers-enhanced-quiet-save-p t)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p nil)
            (auto-save-buffers-enhanced-cooperate-elscreen-p nil))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (add-hook 'before-save-hook
                    (lambda ()
                      (message "quiet before-save Ω")
                      (setq neomacs-asbe-test--events
                            (cons :before neomacs-asbe-test--events)))
                    nil t)
          (add-hook 'after-save-hook
                    (lambda ()
                      (message "quiet after-save λ")
                      (setq neomacs-asbe-test--events
                            (cons :after neomacs-asbe-test--events)))
                    nil t)
          (goto-char (point-max))
          (insert "release: ready Ω\n"))
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (message nil)
        (neomacs-asbe-test--fire-idle-tick)
        (setq first-save
              (with-current-buffer buffer
                (list
                 :disk (neomacs-asbe-test--file-text path)
                 :buffer (neomacs-asbe-test--buffer-state buffer)
                 :save-events (nreverse neomacs-asbe-test--events)
                 :message (current-message)
                 :messages (neomacs-asbe-test--messages messages-start)
                 :visited-modtime (visited-file-modtime)
                 :verified (verify-visited-file-modtime (current-buffer))
                 :message-function-restored
                 (eq (symbol-function 'message) original-message-function)
                 :write-function-restored
                 (eq (symbol-function 'write-region) original-write-function)
                 :global-alias-created
                 (and
                  (fboundp 'original-write-region)
                  (eq (symbol-function 'original-write-region)
                      original-write-function)))))
        (setq neomacs-asbe-test--events nil)
        (setq messages-start (with-current-buffer (messages-buffer) (point-max)))
        (neomacs-asbe-test--write-file path "external replacement\n")
        (with-current-buffer buffer
          (goto-char (point-max))
          (insert "release: local follow-up λ\n"))
        (message nil)
        (neomacs-asbe-test--fire-idle-tick)
        (setq result
              (with-current-buffer buffer
                (list
                 :first-save first-save
                 :after-external-replacement
                 (list
                  :disk (neomacs-asbe-test--file-text path)
                  :buffer (neomacs-asbe-test--buffer-state buffer)
                  :save-events (nreverse neomacs-asbe-test--events)
                  :message (current-message)
                  :messages (neomacs-asbe-test--messages messages-start)
                  :visited-modtime (visited-file-modtime)
                  :verified (verify-visited-file-modtime (current-buffer))
                  :message-function-restored
                  (eq (symbol-function 'message) original-message-function)
                  :write-function-restored
                  (eq (symbol-function 'write-region) original-write-function))))))
    (neomacs-asbe-test--cancel-package-timers)
    (setq neomacs-asbe-test--events nil)
    (if alias-was-bound
        (fset 'original-write-region alias-old-function)
      (when (fboundp 'original-write-region)
        (fmakunbound 'original-write-region)))
    (neomacs-asbe-test--cleanup-buffers (list buffer))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:first-save (:disk "release: draft\nrelease: ready Ω\n" :buffer (:text "release: draft\nrelease: ready Ω\n" :point 33 :modified nil :read-only nil) :save-events (:before :after) :message nil :messages nil :visited-modtime 0 :verified t :message-function-restored t :write-function-restored t :global-alias-created t) :after-external-replacement (:disk "release: draft\nrelease: ready Ω\nrelease: local follow-up λ\n" :buffer (:text "release: draft\nrelease: ready Ω\nrelease: local follow-up λ\n" :point 60 :modified nil :read-only nil) :save-events (:before :after) :message nil :messages nil :visited-modtime 0 :verified t :message-function-restored t :write-function-restored t))"#
    ]];
    ParityBatchCase::value(
        "quiet_save_persists_bytes_runs_hooks_and_clears_visited_modtime",
        elisp_form,
        expect,
    )
}

fn scratch_contents_round_trip_through_the_documented_persistence_file() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-scratch"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "persisted scratch Ω.el" root))
       (scratch-existed (and (get-buffer "*scratch*") t))
       (scratch (get-buffer-create "*scratch*"))
       original-text original-point original-modified
       initial-skipped saved restored missing-file result)
  (make-directory root t)
  (with-current-buffer scratch
    (setq original-text
          (buffer-substring-no-properties (point-min) (point-max))
          original-point (point)
          original-modified (buffer-modified-p)))
  (neomacs-asbe-test--load-package)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps nil)
            (auto-save-buffers-enhanced-exclude-regexps '(".+"))
            (auto-save-buffers-enhanced-activity-flag nil)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p t)
            (auto-save-buffers-enhanced-file-related-with-scratch-buffer path)
            (auto-save-buffers-enhanced-cooperate-elscreen-p t)
            (initial-scratch-message ";; initial scratch\n")
            (after-init-hook nil)
            (elscreen-create-hook nil))
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (with-current-buffer scratch
          (erase-buffer)
          (insert initial-scratch-message)
          (set-buffer-modified-p t))
        (neomacs-asbe-test--fire-idle-tick)
        (setq initial-skipped
              (list
               :file-exists (and (file-exists-p path) t)
               :scratch (neomacs-asbe-test--buffer-state scratch)))
        (with-current-buffer scratch
          (erase-buffer)
          (insert "(message \"deploy Ω\")\nnotes λ\n")
          (goto-char 12)
          (set-buffer-modified-p t))
        (neomacs-asbe-test--fire-idle-tick)
        (setq saved
              (list
               :activity auto-save-buffers-enhanced-activity-flag
               :file (neomacs-asbe-test--file-text path)
               :scratch (neomacs-asbe-test--buffer-state scratch)
               :hook-count
               (neomacs-asbe-test--hook-count
                'auto-save-buffers-enhanced-scratch-read-after-init-hook
                after-init-hook)
               :elscreen-hook-count
               (neomacs-asbe-test--hook-count
                'auto-save-buffers-enhanced-cooperate-elscreen-default-window
                elscreen-create-hook)))
        (with-current-buffer scratch
          (erase-buffer)
          (insert "cleared session\n")
          (set-buffer-modified-p nil))
        (run-hooks 'elscreen-create-hook)
        (setq restored (neomacs-asbe-test--buffer-state scratch))
        (delete-file path)
        (with-current-buffer scratch
          (erase-buffer)
          (insert "keep this session Ω\n")
          (goto-char 6)
          (set-buffer-modified-p nil))
        (run-hooks 'after-init-hook)
        (setq missing-file (neomacs-asbe-test--buffer-state scratch))
        (setq result
              (list
               :initial-message-skipped initial-skipped
               :paused-activity-save saved
               :restored-through-elscreen-alias restored
               :missing-file-keeps-session missing-file)))
    (neomacs-asbe-test--cancel-package-timers)
    (when (buffer-live-p scratch)
      (with-current-buffer scratch
        (setq buffer-read-only nil)
        (erase-buffer)
        (insert original-text)
        (goto-char (min original-point (point-max)))
        (set-buffer-modified-p original-modified)))
    (unless scratch-existed
      (when (buffer-live-p scratch)
        (kill-buffer scratch)))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:initial-message-skipped (:file-exists nil :scratch (:text ";; initial scratch\n" :point 20 :modified t :read-only nil)) :paused-activity-save (:activity nil :file "(message \"deploy Ω\")\nnotes λ\n" :scratch (:text "(message \"deploy Ω\")\nnotes λ\n" :point 12 :modified nil :read-only nil) :hook-count 1 :elscreen-hook-count 1) :restored-through-elscreen-alias (:text "(message \"deploy Ω\")\nnotes λ\n" :point 1 :modified t :read-only nil) :missing-file-keeps-session (:text "keep this session Ω\n" :point 6 :modified nil :read-only nil))"#
    ]];
    ParityBatchCase::value(
        "scratch_contents_round_trip_through_the_documented_persistence_file",
        elisp_form,
        expect,
    )
}

fn undo_after_an_idle_save_is_persisted_by_the_next_idle_tick() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-undo"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (path (expand-file-name "deployment checklist Ω.txt" root))
       buffer saved undo-outcome undone result)
  (neomacs-asbe-test--write-file path "task: draft\n")
  (setq buffer (find-file-noselect path))
  (neomacs-asbe-test--load-package)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps '(".+"))
            (auto-save-buffers-enhanced-exclude-regexps nil)
            (auto-save-buffers-enhanced-activity-flag t)
            (auto-save-buffers-enhanced-quiet-save-p nil)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p nil)
            (auto-save-buffers-enhanced-cooperate-elscreen-p nil))
        (with-current-buffer buffer
          (setq-local make-backup-files nil)
          (setq buffer-undo-list nil)
          (goto-char (point-max))
          (let ((old-window-buffer (window-buffer (selected-window))))
            (unwind-protect
                (progn
                  (set-window-buffer (selected-window) (current-buffer))
                  (execute-kbd-macro "status: ready Ω\n"))
              (set-window-buffer (selected-window) old-window-buffer))))
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (neomacs-asbe-test--fire-idle-tick)
        (setq saved
              (list
               :disk (neomacs-asbe-test--file-text path)
               :buffer (neomacs-asbe-test--buffer-state buffer)))
        (setq undo-outcome
              (with-current-buffer buffer
                (condition-case error-data
                    (progn (undo 1) :ok)
                  (error (list (car error-data) (cdr error-data))))))
        (setq undone
              (list
               :outcome undo-outcome
               :disk (neomacs-asbe-test--file-text path)
               :buffer (neomacs-asbe-test--buffer-state buffer)))
        (neomacs-asbe-test--fire-idle-tick)
        (setq result
              (list
               :saved saved
               :after-undo undone
               :after-next-idle
               (list
                :disk (neomacs-asbe-test--file-text path)
                :buffer (neomacs-asbe-test--buffer-state buffer)))))
    (neomacs-asbe-test--cancel-package-timers)
    (neomacs-asbe-test--cleanup-buffers (list buffer))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:saved (:disk "task: draft\nstatus: ready Ω\n" :buffer (:text "task: draft\nstatus: ready Ω\n" :point 29 :modified nil :read-only nil)) :after-undo (:outcome :ok :disk "task: draft\nstatus: ready Ω\n" :buffer (:text "task: draft\n" :point 13 :modified t :read-only nil)) :after-next-idle (:disk "task: draft\n" :buffer (:text "task: draft\n" :point 13 :modified nil :read-only nil)))"#
    ]];
    ParityBatchCase::value(
        "undo_after_an_idle_save_is_persisted_by_the_next_idle_tick",
        elisp_form,
        expect,
    )
}

fn checkout_only_mode_keeps_outermost_duplicate_rules_and_saves_sibling_files() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-checkout"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (project (file-name-as-directory (expand-file-name "project Ω" root)))
       (inside-path (expand-file-name "src/notes.txt" project))
       (sibling-path (expand-file-name "sibling outside inner.txt" root))
       ;; The package keeps the OUTERMOST checkout ancestor, not the
       ;; innermost one `locate-dominating-file' returns; see
       ;; `neomacs-asbe-test--outermost-checkout'.
       (outer-checkout (neomacs-asbe-test--outermost-checkout root))
       (expected-rule (concat "^" (regexp-quote outer-checkout)))
       inside sibling result)
  (make-directory (expand-file-name ".git" project) t)
  (neomacs-asbe-test--write-file inside-path "inside: old\n")
  (neomacs-asbe-test--write-file sibling-path "sibling: old\n")
  (neomacs-asbe-test--load-package)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-interval 60)
            (auto-save-buffers-enhanced-include-regexps '("discarded"))
            (auto-save-buffers-enhanced-exclude-regexps nil)
            (auto-save-buffers-enhanced-use-svk-flag nil)
            (auto-save-buffers-enhanced-activity-flag t)
            (auto-save-buffers-enhanced-save-scratch-buffer-to-file-p nil)
            (auto-save-buffers-enhanced-cooperate-elscreen-p nil)
            (find-file-hook nil))
        (auto-save-buffers-enhanced-include-only-checkout-path t)
        (setq inside (find-file-noselect inside-path)
              sibling (find-file-noselect sibling-path))
        (dolist (buffer (list inside sibling))
          (with-current-buffer buffer
            (setq-local make-backup-files nil)
            (goto-char (point-max))))
        (with-current-buffer inside (insert "inside: saved Ω\n"))
        (with-current-buffer sibling (insert "sibling: also saved λ\n"))
        (auto-save-buffers-enhanced-include-only-checkout-path nil)
        (neomacs-asbe-test--cancel-package-timers)
        (auto-save-buffers-enhanced t)
        (neomacs-asbe-test--fire-idle-tick)
        (setq result
              (list
               :hook-count
               (neomacs-asbe-test--hook-count
                'auto-save-buffers-enhanced-add-checkout-path-into-include-regexps
                find-file-hook)
               :include-rules-after-two-visits-and-disable
               (mapcar
                (lambda (regexp)
                  (list
                   :shape
                   (if (equal regexp expected-rule)
                       "^<outermost-checkout>/"
                     regexp)
                   :inside (and (string-match-p regexp inside-path) t)
                   :sibling (and (string-match-p regexp sibling-path) t)))
                auto-save-buffers-enhanced-include-regexps)
               :duplicate-rules
               (and
                (= (length auto-save-buffers-enhanced-include-regexps) 2)
                (equal (car auto-save-buffers-enhanced-include-regexps)
                       (cadr auto-save-buffers-enhanced-include-regexps)))
               :inside
               (list
                :disk (neomacs-asbe-test--file-text inside-path)
                :buffer (neomacs-asbe-test--buffer-state inside))
               :sibling
               (list
                :disk (neomacs-asbe-test--file-text sibling-path)
                :buffer (neomacs-asbe-test--buffer-state sibling)))))
    (neomacs-asbe-test--cancel-package-timers)
    (neomacs-asbe-test--cleanup-buffers (list inside sibling))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:hook-count 1 :include-rules-after-two-visits-and-disable ((:shape "^<outermost-checkout>/" :inside t :sibling t) (:shape "^<outermost-checkout>/" :inside t :sibling t)) :duplicate-rules t :inside (:disk "inside: old\ninside: saved Ω\n" :buffer (:text "inside: old\ninside: saved Ω\n" :point 29 :modified nil :read-only nil)) :sibling (:disk "sibling: old\nsibling: also saved λ\n" :buffer (:text "sibling: old\nsibling: also saved λ\n" :point 36 :modified nil :read-only nil)))"#
    ]];
    ParityBatchCase::value(
        "checkout_only_mode_keeps_outermost_duplicate_rules_and_saves_sibling_files",
        elisp_form,
        expect,
    )
}

fn missing_svk_config_reports_success_without_creating_rules_or_a_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-svk-missing"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (config (expand-file-name "missing svk config" root))
       (sentinel (generate-new-buffer " *auto-save-svk-current*"))
       messages-start result)
  (make-directory root t)
  (neomacs-asbe-test--load-package)
  (setq messages-start (with-current-buffer (messages-buffer) (point-max)))
  (unwind-protect
      (let ((auto-save-buffers-enhanced-svk-config-path config)
            (auto-save-buffers-enhanced-include-regexps nil))
        (set-buffer sentinel)
        (call-interactively 'auto-save-buffers-enhanced-reload-svk)
        (setq result
              (list
               :messages
               (mapcar
                (lambda (message)
                  (replace-regexp-in-string
                   (regexp-quote config) "<config>" message t t))
                (neomacs-asbe-test--messages messages-start))
               :current-buffer-preserved (eq (current-buffer) sentinel)
               :config-buffer-alive
               (and (get-file-buffer config) t)
               :config-created (and (file-exists-p config) t)
               :include-rules auto-save-buffers-enhanced-include-regexps)))
    (neomacs-asbe-test--cleanup-buffers (list sentinel))
    (neomacs-asbe-test--cleanup-root root))
  result)
"####;
    let expect = expect![[
        r#"OK (:messages ("Realoaded checkout paths from <config>") :current-buffer-preserved t :config-buffer-alive nil :config-created nil :include-rules nil)"#
    ]];
    ParityBatchCase::value(
        "missing_svk_config_reports_success_without_creating_rules_or_a_buffer",
        elisp_form,
        expect,
    )
}

fn readable_svk_config_surfaces_the_removed_toggle_read_only_command() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-save-svk-readable"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (checkout
        (file-name-as-directory (expand-file-name "checkout Ω" root)))
       (config (expand-file-name "svk config" root)))
  (make-directory checkout t)
  (neomacs-asbe-test--write-file
   config (format "  %s:\n" (directory-file-name checkout)))
  (neomacs-asbe-test--load-package)
  (unwind-protect
      (let ((auto-save-buffers-enhanced-svk-config-path config)
            (auto-save-buffers-enhanced-include-regexps nil))
        (call-interactively 'auto-save-buffers-enhanced-reload-svk))
    (neomacs-asbe-test--cleanup-buffers (list (get-file-buffer config)))
    (neomacs-asbe-test--cleanup-root root)))
"####;
    let expect = expect!["ERR (void-function toggle-read-only)"];
    ParityBatchCase::signal(
        "readable_svk_config_surfaces_the_removed_toggle_read_only_command",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        activation_schedules_real_repeating_idle_timers_and_optional_hooks(),
        idle_tick_saves_only_included_writable_editable_buffers(),
        interactive_activity_toggle_pauses_and_resumes_the_same_timer(),
        quiet_save_persists_bytes_runs_hooks_and_clears_visited_modtime(),
        scratch_contents_round_trip_through_the_documented_persistence_file(),
        undo_after_an_idle_save_is_persisted_by_the_next_idle_tick(),
        checkout_only_mode_keeps_outermost_duplicate_rules_and_saves_sibling_files(),
        missing_svk_config_reports_success_without_creating_rules_or_a_buffer(),
        readable_svk_config_surfaces_the_removed_toggle_read_only_command(),
    ]
}
