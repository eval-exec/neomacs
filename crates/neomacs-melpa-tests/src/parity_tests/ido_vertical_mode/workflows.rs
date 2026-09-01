use expect_test::expect;

use super::ParityBatchCase;

fn buffer_navigation_uses_real_vertical_candidates_and_user_keys() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let (fixtures)
     (unwind-protect
         (progn
           (dolist (entry '(("deploy-alpha" . "ALPHA RUNBOOK\n")
                            ("deploy-beta" . "BETA RUNBOOK\n")
                            ("deploy-notes" . "NOTES RUNBOOK\n")
                            ("deploy-ops" . "OPS RUNBOOK\n")
                            ("deploy-archive" . "ARCHIVE RUNBOOK\n")))
             (let ((buffer (get-buffer-create (car entry))))
               (push buffer fixtures)
               (with-current-buffer buffer
                 (erase-buffer)
                 (insert (cdr entry)))))
           (switch-to-buffer (get-buffer-create "ido-vertical-origin"))
           (minibuffer-with-setup-hook
               (lambda ()
                 (setq unread-command-events
                       (append
                        (string-to-list "deploy-")
                        (listify-key-sequence
                         (kbd "<f8> C-n <f8> <down> <f8> <up> <f8> RET"))
                        unread-command-events)))
             (ido-switch-buffer))
           (list
            :selected (buffer-name)
            :contents (buffer-substring-no-properties (point-min) (point-max))
            :observations (neomacs-ido-vertical-test-finish 4)))
       (mapc (lambda (buffer)
               (when (buffer-live-p buffer)
                 (kill-buffer buffer)))
             fixtures)
       (when-let* ((origin (get-buffer "ido-vertical-origin")))
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:selected "deploy-beta" :contents "BETA RUNBOOK\n" :observations ((:item buffer :prompt "Buffer: " :input "deploy-" :point 7 :display " [5]\n界·deploy-alpha\n  ·deploy-beta\n  ·deploy-notes\n  ·deploy-ops\n  ·..." :face-runs ((7 14 "deploy-" (ido-vertical-first-match-face ido-vertical-match-face)) (14 19 "alpha" ido-vertical-first-match-face) (23 30 "deploy-" ido-vertical-match-face) (38 45 "deploy-" ido-vertical-match-face) (54 61 "deploy-" ido-vertical-match-face)) :matches ("deploy-alpha" "deploy-beta" "deploy-notes" "deploy-ops" "deploy-archive") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "deploy-" :point 7 :display " [5]\n界·deploy-beta\n  ·deploy-notes\n  ·deploy-ops\n  ·deploy-archive\n  ·..." :face-runs ((7 14 "deploy-" (ido-vertical-first-match-face ido-vertical-match-face)) (14 18 "beta" ido-vertical-first-match-face) (22 29 "deploy-" ido-vertical-match-face) (38 45 "deploy-" ido-vertical-match-face) (52 59 "deploy-" ido-vertical-match-face)) :matches ("deploy-beta" "deploy-notes" "deploy-ops" "deploy-archive" "deploy-alpha") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "deploy-" :point 7 :display " [5]\n界·deploy-notes\n  ·deploy-ops\n  ·deploy-archive\n  ·deploy-alpha\n  ·..." :face-runs ((7 14 "deploy-" (ido-vertical-first-match-face ido-vertical-match-face)) (14 19 "notes" ido-vertical-first-match-face) (23 30 "deploy-" ido-vertical-match-face) (37 44 "deploy-" ido-vertical-match-face) (55 62 "deploy-" ido-vertical-match-face)) :matches ("deploy-notes" "deploy-ops" "deploy-archive" "deploy-alpha" "deploy-beta") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "deploy-" :point 7 :display " [5]\n界·deploy-beta\n  ·deploy-notes\n  ·deploy-ops\n  ·deploy-archive\n  ·..." :face-runs ((7 14 "deploy-" (ido-vertical-first-match-face ido-vertical-match-face)) (14 18 "beta" ido-vertical-first-match-face) (22 29 "deploy-" ido-vertical-match-face) (38 45 "deploy-" ido-vertical-match-face) (52 59 "deploy-" ido-vertical-match-face)) :matches ("deploy-beta" "deploy-notes" "deploy-ops" "deploy-archive" "deploy-alpha") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "buffer_navigation_uses_real_vertical_candidates_and_user_keys",
        elisp_form,
        expected,
    )
}

fn relocated_prefix_toggle_changes_a_real_substring_match() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let (fixtures)
     (unwind-protect
         (progn
           (dolist (name '("release-alpha" "release-beta" "release-ops"))
             (push (get-buffer-create name) fixtures))
           (switch-to-buffer (get-buffer-create "ido-prefix-origin"))
           (minibuffer-with-setup-hook
               (lambda ()
                 (setq unread-command-events
                       (append
                        (string-to-list "alpha")
                        (listify-key-sequence
                         (kbd "<f8> C-c C-t <f8> C-c C-t <f8> RET"))
                        unread-command-events)))
             (ido-switch-buffer))
           (list :selected (buffer-name)
                 :observations (neomacs-ido-vertical-test-finish 3)))
       (mapc (lambda (buffer)
               (when (buffer-live-p buffer)
                 (kill-buffer buffer)))
             fixtures)
       (when-let* ((origin (get-buffer "ido-prefix-origin")))
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:selected "release-alpha" :observations ((:item buffer :prompt "Buffer: " :input "alpha" :point 5 :display " [1]\n界·release-alpha\n  ·\n  ·\n  ·\n" :face-runs ((7 15 "release-" ido-vertical-only-match-face) (15 20 "alpha" (ido-vertical-only-match-face ido-vertical-match-face))) :matches ("release-alpha") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "alpha" :point 5 :display " [No match]" :face-runs nil :matches nil :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "alpha" :point 5 :display " [1]\n界·release-alpha\n  ·\n  ·\n  ·\n" :face-runs ((7 15 "release-" ido-vertical-only-match-face) (15 20 "alpha" (ido-vertical-only-match-face ido-vertical-match-face))) :matches ("release-alpha") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "relocated_prefix_toggle_changes_a_real_substring_match",
        elisp_form,
        expected,
    )
}

fn disable_if_short_responds_to_real_minibuffer_width() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let ((ido-vertical-disable-if-short t)
         (ido-vertical-show-count nil)
         (ido-vertical-pad-list nil)
         (ido-use-faces nil)
         fixtures)
     (unwind-protect
         (progn
           (dolist (name
                    (list "short-alpha"
                          "short-beta"
                          (concat "long-" (make-string 100 ?a))
                          (concat "long-" (make-string 100 ?b))))
             (push (get-buffer-create name) fixtures))
           (switch-to-buffer (get-buffer-create "ido-responsive-origin"))
           (condition-case nil
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq unread-command-events
                           (append
                            (string-to-list "short-")
                            (listify-key-sequence (kbd "<f8> C-g"))
                            unread-command-events)))
                 (ido-switch-buffer))
             (quit nil))
           (condition-case nil
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq unread-command-events
                           (append
                            (string-to-list "long-")
                            (listify-key-sequence (kbd "<f8> C-g"))
                            unread-command-events)))
                 (ido-switch-buffer))
             (quit nil))
           (list :window-width (window-body-width (minibuffer-window))
                 :observations (neomacs-ido-vertical-test-finish 2)))
       (mapc (lambda (buffer)
               (when (buffer-live-p buffer)
                 (kill-buffer buffer)))
             fixtures)
       (when-let* ((origin (get-buffer "ido-responsive-origin")))
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:window-width 80 :observations ((:item buffer :prompt "Buffer: " :input "short-" :point 6 :display "{short-alpha | short-beta}" :face-runs nil :matches ("short-alpha" "short-beta") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "long-" :point 5 :display "\n界·long-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  ·long-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" :face-runs nil :matches ("long-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "long-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "disable_if_short_responds_to_real_minibuffer_width",
        elisp_form,
        expected,
    )
}

fn incomplete_regexp_renders_and_recovers_in_a_real_prompt() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let (fixtures)
     (unwind-protect
         (progn
           (dolist (name '("regex-alpha" "regex-beta"))
             (push (get-buffer-create name) fixtures))
           (switch-to-buffer (get-buffer-create "ido-regexp-origin"))
           (minibuffer-with-setup-hook
               (lambda ()
                 (setq unread-command-events
                       (append
                        (string-to-list "regex-")
                        (listify-key-sequence (kbd "C-t"))
                        (string-to-list "[")
                        (listify-key-sequence (kbd "<f8> DEL RET"))
                        unread-command-events)))
             (ido-switch-buffer))
           (list :selected (buffer-name)
                 :observations (neomacs-ido-vertical-test-finish 1)))
       (mapc (lambda (buffer)
               (when (buffer-live-p buffer)
                 (kill-buffer buffer)))
             fixtures)
       (when-let* ((origin (get-buffer "ido-regexp-origin")))
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:selected "regex-alpha" :observations ((:item buffer :prompt "Buffer: " :input "regex-[" :point 7 :display " Unmatched [ or [^" :face-runs ((1 18 "Unmatched [ or [^" ido-incomplete-regexp)) :matches ("Unmatched [ or [^") :regexp t :incomplete-regexp t :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "incomplete_regexp_renders_and_recovers_in_a_real_prompt",
        elisp_form,
        expected,
    )
}

fn nested_project_file_selection_preserves_vertical_file_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let* ((root
           (file-name-as-directory
            (expand-file-name
             "ido-vertical-project"
             (or (getenv "NEOMACS_TEST_SANDBOX_ROOT")
                 (error "NEOMACS_TEST_SANDBOX_ROOT is required")))))
          (configs (expand-file-name "configs/" root))
          (neomacs-ido-vertical-test-project-root root)
          selected)
     (unwind-protect
         (progn
           (make-directory configs t)
           (dolist (entry '(("service-api.py" . "print('API READY')\n")
                            ("service-worker.py" . "print('WORKER READY')\n")
                            ("service-web.py" . "print('WEB READY')\n")))
             (with-temp-file (expand-file-name (car entry) configs)
               (insert (cdr entry))))
           (let ((default-directory root))
             (let ((ido-file-history '("service-worker.py")))
               (condition-case nil
                   (minibuffer-with-setup-hook
                       (lambda ()
                         (setq unread-command-events
                               (append
                                (string-to-list "conf")
                                (listify-key-sequence (kbd "TAB"))
                                (string-to-list "service-")
                                (listify-key-sequence
                                 (kbd "<f8> <left> <f8> C-g"))
                                unread-command-events)))
                     (ido-find-file))
                 (quit nil)))
             (minibuffer-with-setup-hook
                 (lambda ()
                   (setq unread-command-events
                         (append
                          (string-to-list "conf")
                          (listify-key-sequence (kbd "TAB"))
                          (string-to-list "service-")
                          (listify-key-sequence
                           (kbd "<f8> <down> <f8> RET"))
                          unread-command-events)))
               (ido-find-file)))
           (setq selected (buffer-file-name))
           (list
            :selected (file-relative-name selected root)
            :contents (buffer-substring-no-properties (point-min) (point-max))
            :observations (neomacs-ido-vertical-test-finish 4)))
       (dolist (name '("service-api.py" "service-worker.py" "service-web.py"))
         (when-let* ((buffer (get-file-buffer (expand-file-name name configs))))
           (kill-buffer buffer)))
       (when (file-directory-p root)
         (delete-directory root t))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:selected "configs/service-web.py" :contents "print('WEB READY')\n" :observations ((:item file :prompt "Find file: .../configs/" :input "service-" :point 8 :display " [3]\n界·service-api.py\n  ·service-web.py\n  ·service-worker.py\n  ·\n" :face-runs ((7 15 "service-" (ido-vertical-first-match-face ido-vertical-match-face)) (15 21 "api.py" ido-vertical-first-match-face) (25 33 "service-" ido-vertical-match-face) (43 51 "service-" ido-vertical-match-face)) :matches ("service-api.py" "service-web.py" "service-worker.py") :regexp nil :incomplete-regexp nil :directory "configs/" :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item file :prompt "Find file: .../configs/" :input "service-worker.py" :point 17 :display " [1]\n界·service-worker.py\n  ·\n  ·\n  ·\n" :face-runs ((7 24 "service-worker.py" (ido-vertical-only-match-face ido-vertical-match-face))) :matches ("service-worker.py") :regexp nil :incomplete-regexp nil :directory "configs/" :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item file :prompt "Find file: .../configs/" :input "service-" :point 8 :display " [3]\n界·service-api.py\n  ·service-web.py\n  ·service-worker.py\n  ·\n" :face-runs ((7 15 "service-" (ido-vertical-first-match-face ido-vertical-match-face)) (15 21 "api.py" ido-vertical-first-match-face) (25 33 "service-" ido-vertical-match-face) (43 51 "service-" ido-vertical-match-face)) :matches ("service-api.py" "service-web.py" "service-worker.py") :regexp nil :incomplete-regexp nil :directory "configs/" :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item file :prompt "Find file: .../configs/" :input "service-" :point 8 :display " [3]\n界·service-web.py\n  ·service-worker.py\n  ·service-api.py\n  ·\n" :face-runs ((7 15 "service-" (ido-vertical-first-match-face ido-vertical-match-face)) (15 21 "web.py" ido-vertical-first-match-face) (25 33 "service-" ido-vertical-match-face) (46 54 "service-" ido-vertical-match-face)) :matches ("service-web.py" "service-worker.py" "service-api.py") :regexp nil :incomplete-regexp nil :directory "configs/" :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "nested_project_file_selection_preserves_vertical_file_workflow",
        elisp_form,
        expected,
    )
}

fn no_match_confirmation_and_abort_recover_without_state_leaks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let ((origin (get-buffer-create "ido-confirm-origin"))
         created
         cancelled)
     (unwind-protect
         (progn
           (switch-to-buffer origin)
           (minibuffer-with-setup-hook
               (lambda ()
                 (setq unread-command-events
                       (append
                        (string-to-list "incident-new")
                        (listify-key-sequence (kbd "<f8> RET RET"))
                        unread-command-events)))
             (ido-switch-buffer))
           (setq created (current-buffer))
           (switch-to-buffer origin)
           (setq cancelled
                 (condition-case condition
                     (minibuffer-with-setup-hook
                         (lambda ()
                           (setq unread-command-events
                                 (append
                                  (string-to-list "deploy-abort")
                                  (listify-key-sequence (kbd "<f8> C-g"))
                                  unread-command-events)))
                       (ido-switch-buffer))
                   (quit (list :signal (car condition)
                               :current-buffer (buffer-name)))))
           (list :created (buffer-name created)
                 :cancelled cancelled
                 :observations (neomacs-ido-vertical-test-finish 3)))
       (when-let* ((created-buffer (get-buffer "incident-new")))
         (kill-buffer created-buffer))
       (when (buffer-live-p origin)
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:created "incident-new" :cancelled (:signal minibuffer-quit :current-buffer "ido-confirm-origin") :observations ((:item buffer :prompt "Buffer: " :input "incident-new" :point 12 :display " [No match]" :face-runs nil :matches nil :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "incident-new" :point 12 :display " [Confirm]" :face-runs nil :matches nil :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "deploy-abort" :point 12 :display " [No match]" :face-runs nil :matches nil :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "no_match_confirmation_and_abort_recover_without_state_leaks",
        elisp_form,
        expected,
    )
}

fn public_mode_lifecycle_restores_horizontal_ido_and_reenables_vertical() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-ido-vertical-test-call
 (lambda ()
   (let (fixtures)
     (unwind-protect
         (progn
           (dolist (name '("audit-alpha" "audit-beta" "audit-notes"))
             (push (get-buffer-create name) fixtures))
           (switch-to-buffer (get-buffer-create "ido-lifecycle-origin"))
           (condition-case nil
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq unread-command-events
                           (append
                            (string-to-list "audit-")
                            (listify-key-sequence (kbd "<f8> C-g"))
                            unread-command-events)))
                 (ido-switch-buffer))
             (quit nil))
           (ido-vertical-mode -1)
           (condition-case nil
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq unread-command-events
                           (append
                            (string-to-list "audit-")
                            (listify-key-sequence (kbd "<f8> C-g"))
                            unread-command-events)))
                 (ido-switch-buffer))
             (quit nil))
           (ido-vertical-mode 1)
           (condition-case nil
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq unread-command-events
                           (append
                            (string-to-list "audit-")
                            (listify-key-sequence (kbd "<f8> C-g"))
                            unread-command-events)))
                 (ido-switch-buffer))
             (quit nil))
           (list :mode-enabled ido-vertical-mode
                 :observations (neomacs-ido-vertical-test-finish 3)))
       (mapc (lambda (buffer)
               (when (buffer-live-p buffer)
                 (kill-buffer buffer)))
             fixtures)
       (when-let* ((origin (get-buffer "ido-lifecycle-origin")))
         (kill-buffer origin))))))
"####;
    let expected = expect![[
        r#"OK (:workflow (:mode-enabled t :observations ((:item buffer :prompt "Buffer: " :input "audit-" :point 6 :display " [3]\n界·audit-alpha\n  ·audit-beta\n  ·audit-notes\n  ·\n" :face-runs ((7 13 "audit-" (ido-vertical-first-match-face ido-vertical-match-face)) (13 18 "alpha" ido-vertical-first-match-face) (22 28 "audit-" ido-vertical-match-face) (36 42 "audit-" ido-vertical-match-face)) :matches ("audit-alpha" "audit-beta" "audit-notes") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil) (:item buffer :prompt "Buffer: " :input "audit-" :point 6 :display "{audit-alpha | audit-beta | audit-notes}" :face-runs ((1 12 "audit-alpha" ido-first-match)) :matches ("audit-alpha" "audit-beta" "audit-notes") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . next-line) ("C-p" . ido-toggle-prefix) ("<up>" . previous-line-or-history-element) ("<down>" . next-line-or-history-element) ("<left>" . ido-prev-match) ("<right>" . ido-next-match) ("C-c C-t")) :message nil) (:item buffer :prompt "Buffer: " :input "audit-" :point 6 :display " [3]\n界·audit-alpha\n  ·audit-beta\n  ·audit-notes\n  ·\n" :face-runs ((7 13 "audit-" (ido-vertical-first-match-face ido-vertical-match-face)) (13 18 "alpha" ido-vertical-first-match-face) (22 28 "audit-" ido-vertical-match-face) (36 42 "audit-" ido-vertical-match-face)) :matches ("audit-alpha" "audit-beta" "audit-notes") :regexp nil :incomplete-regexp nil :directory nil :truncate-lines nil :keys (("C-n" . ido-next-match) ("C-p" . ido-prev-match) ("<up>" . ido-prev-match) ("<down>" . ido-next-match) ("<left>" . ido-vertical-prev-match) ("<right>" . ido-vertical-next-match) ("C-c C-t" . ido-toggle-prefix)) :message nil))) :cleanup (:ido-mode nil :ido-vertical-mode nil :renderer restored :decorations restored :hooks restored :events empty :minibuffer inactive))"#
    ]];
    ParityBatchCase::value(
        "public_mode_lifecycle_restores_horizontal_ido_and_reenables_vertical",
        elisp_form,
        expected,
    )
}

fn disabling_an_already_disabled_mode_preserves_pinned_failure_boundary() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-completions (symbol-function 'ido-completions))
      (original-decorations (copy-tree ido-decorations))
      (original-setup-hook (copy-sequence ido-setup-hook))
      (original-minibuffer-hook (copy-sequence ido-minibuffer-setup-hook)))
  (unwind-protect
      (list
       :before (list :mode ido-vertical-mode
                     :renderer (and (fboundp 'ido-completions) 'defined)
                     :decorations (and ido-decorations 'defined))
       :return (ido-vertical-mode -1)
       :after (list :mode ido-vertical-mode
                    :renderer (and (fboundp 'ido-completions) 'defined)
                    :function (symbol-function 'ido-completions)
                    :decorations ido-decorations))
    (fset 'ido-completions original-completions)
    (setq ido-decorations original-decorations
          ido-vertical-mode nil)
    (setq ido-setup-hook original-setup-hook
          ido-minibuffer-setup-hook original-minibuffer-hook)))
"####;
    let expected = expect![
        "OK (:before (:mode nil :renderer defined :decorations defined) :return nil :after (:mode nil :renderer nil :function nil :decorations nil))"
    ];
    ParityBatchCase::value(
        "disabling_an_already_disabled_mode_preserves_pinned_failure_boundary",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        buffer_navigation_uses_real_vertical_candidates_and_user_keys(),
        relocated_prefix_toggle_changes_a_real_substring_match(),
        disable_if_short_responds_to_real_minibuffer_width(),
        incomplete_regexp_renders_and_recovers_in_a_real_prompt(),
        nested_project_file_selection_preserves_vertical_file_workflow(),
        no_match_confirmation_and_abort_recover_without_state_leaks(),
        public_mode_lifecycle_restores_horizontal_ido_and_reenables_vertical(),
        disabling_an_already_disabled_mode_preserves_pinned_failure_boundary(),
    ]
}
