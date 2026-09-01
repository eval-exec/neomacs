use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_COLLECTION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(setq evil-want-integration t
      evil-want-keybinding nil)
(require 'evil-collection)
(evil-mode 1)

(defun neomacs-evil-collection-test-execute (keys)
  "Execute KEYS through the current buffer's active Evil keymaps."
  (execute-kbd-macro (kbd keys)))

(defun neomacs-evil-collection-test-command (map state keys)
  "Return the command installed for KEYS in STATE's auxiliary MAP."
  (let ((state-map (evil-get-auxiliary-keymap map state)))
    (and state-map (lookup-key state-map (kbd keys)))))

(defun neomacs-evil-collection-test-use-buffer (buffer)
  "Display BUFFER and enter Evil normal state there."
  (set-window-buffer (selected-window) buffer)
  (set-buffer buffer)
  (evil-local-mode 1)
  (evil-normal-state))
"####;

fn deferred_calendar_setup_drives_a_real_date_planning_session() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-collection-want-unimpaired-p nil)
      (setup-events nil))
  (add-hook 'evil-collection-setup-hook
            (lambda (mode maps &rest _)
              (push (list mode maps) setup-events)))
  (let ((loaded-before (featurep 'calendar)))
    (evil-collection-init 'calendar)
    (let ((registered-before-load
           (not (null (assq 'calendar after-load-alist)))))
      (require 'calendar)
      (let ((calendar-buffer "*evil-collection-release-calendar*"))
        (calendar-basic-setup nil t)
        (let ((buffer (get-buffer calendar-buffer)))
          (unwind-protect
              (progn
                (neomacs-evil-collection-test-use-buffer buffer)
                (calendar-goto-date '(8 5 2026))
                (let ((start (calendar-cursor-to-date t))
                      (bindings
                       (mapcar
                        (lambda (key)
                          (cons key
                                (neomacs-evil-collection-test-command
                                 calendar-mode-map 'normal key)))
                        '("h" "j" "k" "l" "[[" "]]" "q" "gr"))))
                  (neomacs-evil-collection-test-execute "l")
                  (let ((after-day (calendar-cursor-to-date t)))
                    (neomacs-evil-collection-test-execute "j")
                    (let ((after-week (calendar-cursor-to-date t)))
                      (neomacs-evil-collection-test-execute "k")
                      (neomacs-evil-collection-test-execute "h")
                      (list :loaded-before loaded-before
                            :registered-before-load registered-before-load
                            :loaded-after (featurep 'calendar)
                            :setup-events (nreverse setup-events)
                            :mode major-mode
                            :evil-state evil-state
                            :start start
                            :after-day after-day
                            :after-week after-week
                            :round-trip (calendar-cursor-to-date t)
                            :bindings bindings)))))
            (when (buffer-live-p buffer) (kill-buffer buffer))))))))
"####;
    let expected = expect![[
        r#"OK (:loaded-before nil :registered-before-load t :loaded-after t :setup-events ((calendar #1=(calendar-mode-map)) (calendar #1#)) :mode calendar-mode :evil-state normal :start (8 5 2026) :after-day (8 6 2026) :after-week (8 13 2026) :round-trip (8 5 2026) :bindings (("h" . calendar-backward-day) ("j" . calendar-forward-week) ("k" . calendar-backward-week) ("l" . calendar-forward-day) ("[[" . calendar-backward-year) ("]]" . calendar-forward-year) ("q" . calendar-exit) ("gr" . calendar-redraw)))"#
    ]];
    ParityBatchCase::value(
        "deferred_calendar_setup_drives_a_real_date_planning_session",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn dired_keys_navigate_and_curate_a_release_directory() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((evil-collection-want-unimpaired-p nil)
       (root (make-temp-file "evil-collection-dired-" t))
       (alpha (expand-file-name "alpha.txt" root))
       (beta (expand-file-name "beta.log" root))
       (docs (expand-file-name "docs" root))
       buffer)
  (unwind-protect
      (progn
        (with-temp-file alpha (insert "alpha\n"))
        (with-temp-file beta (insert "beta\n"))
        (make-directory docs)
        (evil-collection-init 'dired)
        (setq buffer (dired-noselect root))
        (neomacs-evil-collection-test-use-buffer buffer)
        (dired-goto-file alpha)
        (let ((start (file-name-nondirectory (dired-get-filename nil t)))
              (bindings
               (mapcar
                (lambda (key)
                  (cons key
                        (neomacs-evil-collection-test-command
                         dired-mode-map 'normal key)))
                '("j" "k" "m" "u" "d" "x" "r" "RET"))))
          (neomacs-evil-collection-test-execute "m")
          (let ((after-first-mark
                 (file-name-nondirectory (dired-get-filename nil t))))
            (neomacs-evil-collection-test-execute "m")
            (let ((marked
                   (sort (mapcar #'file-name-nondirectory
                                 (dired-get-marked-files nil nil))
                         #'string<)))
              (neomacs-evil-collection-test-execute "k")
              (let ((after-up
                     (file-name-nondirectory (dired-get-filename nil t))))
                (neomacs-evil-collection-test-execute "u")
                (list :mode major-mode
                      :evil-state evil-state
                      :start start
                      :after-first-mark after-first-mark
                      :marked-before-unmark marked
                      :after-up after-up
                      :marked-after-unmark
                      (sort (mapcar #'file-name-nondirectory
                                    (dired-get-marked-files nil nil))
                            #'string<)
                      :point-file
                      (file-name-nondirectory (dired-get-filename nil t))
                      :bindings bindings))))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (delete-directory root t)))
"####;
    let expected = expect![[
        r#"OK (:mode dired-mode :evil-state normal :start "alpha.txt" :after-first-mark "beta.log" :marked-before-unmark ("alpha.txt" "beta.log") :after-up "beta.log" :marked-after-unmark ("alpha.txt") :point-file "docs" :bindings (("j" . dired-next-line) ("k" . dired-previous-line) ("m" . dired-mark) ("u" . dired-unmark) ("d" . dired-flag-file-deletion) ("x" . dired-do-flagged-delete) ("r" . dired-do-redisplay) ("RET" . dired-find-file)))"#
    ]];
    ParityBatchCase::value(
        "dired_keys_navigate_and_curate_a_release_directory",
        elisp_form,
        expected,
    )
}

fn help_keys_traverse_and_activate_documentation_buttons() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-collection-want-unimpaired-p nil)
      (actions nil)
      (buffer (generate-new-buffer " *evil-collection-help*")))
  (unwind-protect
      (progn
        (evil-collection-init 'help)
        (with-current-buffer buffer
          (help-mode)
          (let ((inhibit-read-only t))
            (insert "Release handbook\n\n")
            (insert-text-button
             "Build artifacts"
             'action (lambda (_) (push 'build actions)))
            (insert " then ")
            (insert-text-button
             "Publish safely"
             'action (lambda (_) (push 'publish actions)))
            (insert "\n")))
        (neomacs-evil-collection-test-use-buffer buffer)
        (goto-char (point-min))
        (neomacs-evil-collection-test-execute "g]")
        (let ((first (button-label (button-at (point)))))
          (neomacs-evil-collection-test-execute "RET")
          (neomacs-evil-collection-test-execute "g]")
          (let ((second (button-label (button-at (point)))))
            (neomacs-evil-collection-test-execute "RET")
            (neomacs-evil-collection-test-execute "g[")
            (neomacs-evil-collection-test-execute "a")
            (list :mode major-mode
                  :read-only buffer-read-only
                  :evil-state evil-state
                  :first first
                  :second second
                  :back-at (button-label (button-at (point)))
                  :actions (nreverse actions)
                  :text (buffer-substring-no-properties
                         (point-min) (point-max))
                  :bindings
                  (mapcar
                   (lambda (key)
                     (cons key
                           (neomacs-evil-collection-test-command
                            help-mode-map 'normal key)))
                   '("g]" "g[" "RET" "q" "a"))))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"####;
    let expected = expect![[
        r#"OK (:mode help-mode :read-only t :evil-state normal :first "Build artifacts" :second "Publish safely" :back-at "Build artifacts" :actions (build publish) :text "Release handbook\n\nBuild artifacts then Publish safely\n" :bindings (("g]" . forward-button) ("g[" . backward-button) ("RET") ("q" . quit-window) ("a")))"#
    ]];
    ParityBatchCase::value(
        "help_keys_traverse_and_activate_documentation_buttons",
        elisp_form,
        expected,
    )
}

fn compilation_keys_walk_diagnostics_by_error_and_source_file() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-collection-want-unimpaired-p nil)
      (buffer (generate-new-buffer " *evil-collection-compile*")))
  (unwind-protect
      (progn
        (evil-collection-init 'compile)
        (with-current-buffer buffer
          (compilation-mode)
          (let ((inhibit-read-only t))
            (insert
             "src/main.rs:10:2: error: invalid release id\n"
             "src/main.rs:18:4: warning: retry is deprecated\n"
             "src/worker.rs:7:1: error: queue is closed\n"))
          (setq buffer-read-only t)
          (goto-char (point-min)))
        (neomacs-evil-collection-test-use-buffer buffer)
        (let ((positions nil))
          (dolist (keys '("gj" "gj" "gk" "]]" "[["))
            (neomacs-evil-collection-test-execute keys)
            (push (list keys
                        (line-number-at-pos)
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
                  positions))
          (list :mode major-mode
                :evil-state evil-state
                :positions (nreverse positions)
                :bindings
                (mapcar
                 (lambda (key)
                   (cons key
                         (neomacs-evil-collection-test-command
                          compilation-mode-map 'normal key)))
                 '("gj" "gk" "]]" "[[" "RET" "gr")))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"####;
    let expected = expect![[
        r#"OK (:mode compilation-mode :evil-state normal :positions (("gj" 2 "src/main.rs:18:4: warning: retry is deprecated") ("gj" 3 "src/worker.rs:7:1: error: queue is closed") ("gk" 2 "src/main.rs:18:4: warning: retry is deprecated") ("]]" 3 "src/worker.rs:7:1: error: queue is closed") ("[[" 2 "src/main.rs:18:4: warning: retry is deprecated")) :bindings (("gj" . compilation-next-error) ("gk" . compilation-previous-error) ("]]" . compilation-next-file) ("[[" . compilation-previous-file) ("RET" . compile-goto-error) ("gr" . recompile)))"#
    ]];
    ParityBatchCase::value(
        "compilation_keys_walk_diagnostics_by_error_and_source_file",
        elisp_form,
        expected,
    )
}

fn user_overrides_rebind_one_action_across_normal_and_insert_workflows() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-collection-binding-overrides
       '((action :state (normal insert) :key ("x" "X"))
         (quit :enabled nil)))
      (evil-collection-key-whitelist nil)
      (evil-collection-key-blacklist nil)
      (events nil)
      (buffer (generate-new-buffer " *evil-collection-console*")))
  (defvar neomacs-evil-collection-console-mode-map (make-sparse-keymap))
  (define-derived-mode neomacs-evil-collection-console-mode special-mode
    "Release-Console")
  (fset 'neomacs-evil-collection-preview
        (lambda () (interactive) (push (list evil-state (point)) events)))
  (evil-collection-bind 'neomacs-evil-collection-console-mode-map
                        'action 'neomacs-evil-collection-preview
                        'quit 'quit-window)
  (unwind-protect
      (progn
        (with-current-buffer buffer
          (neomacs-evil-collection-console-mode)
          (let ((inhibit-read-only t)) (insert "release candidate")))
        (neomacs-evil-collection-test-use-buffer buffer)
        (goto-char 9)
        (neomacs-evil-collection-test-execute "x")
        (evil-insert-state)
        (neomacs-evil-collection-test-execute "X")
        (list :events (nreverse events)
              :state evil-state
              :text (buffer-substring-no-properties (point-min) (point-max))
              :normal
              (mapcar
               (lambda (key)
                 (cons key
                       (neomacs-evil-collection-test-command
                        neomacs-evil-collection-console-mode-map 'normal key)))
               '("x" "X" "q" "RET"))
              :insert
              (mapcar
               (lambda (key)
                 (cons key
                       (neomacs-evil-collection-test-command
                        neomacs-evil-collection-console-mode-map 'insert key)))
               '("x" "X" "q" "RET"))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"####;
    let expected = expect![[
        r#"OK (:events ((normal 9) (insert 9)) :state insert :text "release candidate" :normal (("x" . neomacs-evil-collection-preview) ("X" . neomacs-evil-collection-preview) ("q") ("RET")) :insert (("x" . neomacs-evil-collection-preview) ("X" . neomacs-evil-collection-preview) ("q") ("RET")))"#
    ]];
    ParityBatchCase::value(
        "user_overrides_rebind_one_action_across_normal_and_insert_workflows",
        elisp_form,
        expected,
    )
}

fn team_key_policy_filters_prefixes_states_and_preserves_allowed_actions() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-collection-key-blacklist '("q" "g"))
      (evil-collection-key-whitelist '("gj"))
      (evil-collection-state-passlist '(normal insert))
      (evil-collection-state-denylist '(insert))
      (events nil)
      (buffer (generate-new-buffer " *evil-collection-policy*")))
  (defvar neomacs-evil-collection-policy-mode-map (make-sparse-keymap))
  (define-derived-mode neomacs-evil-collection-policy-mode special-mode
    "Release-Policy")
  (fset 'neomacs-evil-collection-policy-action
        (lambda () (interactive) (push 'allowed events)))
  (evil-collection-define-key '(normal insert visual)
      'neomacs-evil-collection-policy-mode-map
    "q" 'neomacs-evil-collection-policy-action
    "gj" 'neomacs-evil-collection-policy-action
    "gk" 'neomacs-evil-collection-policy-action
    "x" 'neomacs-evil-collection-policy-action)
  (unwind-protect
      (progn
        (with-current-buffer buffer (neomacs-evil-collection-policy-mode))
        (neomacs-evil-collection-test-use-buffer buffer)
        (neomacs-evil-collection-test-execute "gj")
        (list :events (nreverse events)
              :can-bind
              (mapcar (lambda (key)
                        (cons key (evil-collection-can-bind-key (kbd key))))
                      '("q" "g" "gj" "gk" "x"))
              :normal
              (mapcar
               (lambda (key)
                 (cons key
                       (neomacs-evil-collection-test-command
                        neomacs-evil-collection-policy-mode-map 'normal key)))
               '("q" "gj" "gk" "x"))
              :insert
              (mapcar
               (lambda (key)
                 (cons key
                       (neomacs-evil-collection-test-command
                        neomacs-evil-collection-policy-mode-map 'insert key)))
               '("q" "gj" "gk" "x"))
              :visual
              (mapcar
               (lambda (key)
                 (cons key
                       (neomacs-evil-collection-test-command
                        neomacs-evil-collection-policy-mode-map 'visual key)))
               '("q" "gj" "gk" "x"))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"####;
    let expected = expect![[
        r#"OK (:events (allowed) :can-bind (("q") ("g") ("gj" "gj") ("gk") ("x" . t)) :normal (("q") ("gj" . neomacs-evil-collection-policy-action) ("gk") ("x" . neomacs-evil-collection-policy-action)) :insert (("q") ("gj") ("gk") ("x")) :visual (("q") ("gj") ("gk") ("x")))"#
    ]];
    ParityBatchCase::value(
        "team_key_policy_filters_prefixes_states_and_preserves_allowed_actions",
        elisp_form,
        expected,
    )
}

fn push_action_dispatches_buttons_without_losing_the_mode_fallback() -> ParityBatchCase {
    let elisp_form = r####"
(let ((events nil))
  (fset 'neomacs-evil-collection-fallback
        (lambda () (interactive) (push 'fallback events)))
  (evil-collection-define-push-action
    neomacs-evil-collection-open-or-fallback
    neomacs-evil-collection-fallback)
  (with-temp-buffer
    (special-mode)
    (let ((inhibit-read-only t))
      (insert "Deployment: ")
      (insert-text-button
       "open runbook"
       'action (lambda (_) (push 'button events)))
      (insert "\nNo link on this line.\n"))
    (goto-char (point-min))
    (search-forward "runbook")
    (backward-char 1)
    (call-interactively #'neomacs-evil-collection-open-or-fallback)
    (forward-line 1)
    (call-interactively #'neomacs-evil-collection-open-or-fallback)
    (list :events (nreverse events)
          :mode major-mode
          :read-only buffer-read-only
          :point-line (line-number-at-pos)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :interactive
          (commandp #'neomacs-evil-collection-open-or-fallback))))
"####;
    let expected = expect![[
        r#"OK (:events (button fallback) :mode special-mode :read-only t :point-line 2 :text "Deployment: open runbook\nNo link on this line.\n" :interactive t)"#
    ]];
    ParityBatchCase::value(
        "push_action_dispatches_buttons_without_losing_the_mode_fallback",
        elisp_form,
        expected,
    )
}

fn evil_collection_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_COLLECTION_MELPA_PIN, "evil-collection.el")
        .expect("prepare pinned Evil Collection source and Evil below ./tmp")
        .with_timeout(Duration::from_secs(300))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_collection_practical_workflows_batch() {
    let cases = vec![
        deferred_calendar_setup_drives_a_real_date_planning_session(),
        dired_keys_navigate_and_curate_a_release_directory(),
        help_keys_traverse_and_activate_documentation_buttons(),
        compilation_keys_walk_diagnostics_by_error_and_source_file(),
        user_overrides_rebind_one_action_across_normal_and_insert_workflows(),
        team_key_policy_filters_prefixes_states_and_preserves_allowed_actions(),
        push_action_dispatches_buttons_without_losing_the_mode_fallback(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("evil-collection practical workflows parity batch");
    assert_oracle_batch_cases(
        evil_collection_oracle(),
        test_name,
        "evil-collection parity",
        &cases,
    );
}
