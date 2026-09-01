use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SMEX_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'smex)

(defun neomacs-smex-test-items (commands)
  "Return COMMANDS with their current Smex counters."
  (mapcar
   (lambda (command)
     (let ((item (assq command smex-cache)))
       (cons command (and item (cdr item)))))
   commands))
"###;

fn package_contract_exposes_the_m_x_workflow_and_user_policy() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'smex package-alist))))
  (cl-progv '(ido-cur-item) '(list)
    (ido-setup-completion-map)
    (let ((ido-completion-map (copy-keymap ido-completion-map)))
      (smex-prepare-ido-bindings)
      (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :feature (featurep 'smex))
     :defaults
     (list smex-auto-update smex-history-length smex-prompt-string
           smex-flex-matching (file-name-nondirectory smex-save-file))
     :completion-keys
     (mapcar (lambda (key) (lookup-key ido-completion-map (kbd key)))
             '("TAB" "C-h f" "C-h w" "M-." "C-a"))
     :prompts
     (mapcar
      (lambda (prefix)
        (let ((current-prefix-arg prefix)) (smex-prompt-with-prefix-arg)))
       '(nil - 3 (4) (16)))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name smex :version "20151212.2209" :requirements ((emacs (24))) :feature t) :defaults (t 7 "M-x " t "smex-items") :completion-keys (minibuffer-complete smex-describe-function smex-where-is smex-find-function move-beginning-of-line) :prompts ("M-x " "- M-x " "3 M-x " "C-u M-x " "16 M-x "))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_the_m_x_workflow_and_user_policy",
        elisp_form,
        expected,
    )
}

fn initialization_discovers_commands_and_refreshes_after_the_command_set_changes() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (save-file (expand-file-name "smex-discovery-state" root)))
  (cl-progv
      '(smex-save-file smex-initialized-p smex-cache smex-ido-cache
        smex-data smex-history smex-command-count kill-emacs-hook ido-mode
        minibuffer-setup-hook)
      (list save-file nil nil nil nil nil 0 (copy-sequence kill-emacs-hook)
            nil (copy-sequence minibuffer-setup-hook))
    (unwind-protect
        (progn
        (fset 'neomacs-smex-test-alpha
              (lambda () (interactive) :alpha))
        (fset 'neomacs-smex-test-beta
              (lambda () (interactive) :beta))
        (fset 'neomacs-smex-test-helper
              (lambda () :not-a-command))
        (when (file-exists-p smex-save-file) (delete-file smex-save-file))
        (smex-initialize)
        (let ((initial
               (list :initialized smex-initialized-p
                     :hook (and (memq 'smex-save-to-file kill-emacs-hook) t)
                     :ido-setup
                     (list :mode ido-mode
                           :hook (and (memq 'ido-minibuffer-setup
                                            minibuffer-setup-hook)
                                      t))
                     :commands
                     (mapcar (lambda (command)
                               (and (assq command smex-cache) t))
                             '(neomacs-smex-test-alpha
                               neomacs-smex-test-beta
                               neomacs-smex-test-helper))
                     :ido-cache
                     (mapcar (lambda (name)
                               (and (member name smex-ido-cache) t))
                             '("neomacs-smex-test-alpha"
                               "neomacs-smex-test-beta"
                               "neomacs-smex-test-helper")))))
          (fset 'neomacs-smex-test-gamma
                (lambda () (interactive) :gamma))
          (let ((changed (and (smex-detect-new-commands) t)))
            (when changed (smex-update))
            (list
             :initial initial
             :refresh
             (list :changed changed
                   :cache (and (assq 'neomacs-smex-test-gamma smex-cache) t)
                   :ido (and (member "neomacs-smex-test-gamma"
                                     smex-ido-cache)
                              t)
                   :history-length (length smex-history))))))
      (mapc (lambda (symbol) (when (fboundp symbol) (fmakunbound symbol)))
            '(neomacs-smex-test-alpha neomacs-smex-test-beta
              neomacs-smex-test-gamma neomacs-smex-test-helper))
      (when (file-exists-p smex-save-file) (delete-file smex-save-file)))))
"###;
    let expected = expect![
        "OK (:initial (:initialized t :hook t :ido-setup (:mode nil :hook t) :commands (t t nil) :ido-cache (t t nil)) :refresh (:changed t :cache t :ido t :history-length 7))"
    ];
    ParityBatchCase::value(
        "initialization_discovers_commands_and_refreshes_after_the_command_set_changes",
        elisp_form,
        expected,
    )
}

fn selecting_commands_executes_with_prefix_and_promotes_real_usage() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((events nil)
       (items
        (list (cons 'neomacs-smex-test-build 5)
              (cons 'neomacs-smex-test-review 2)
              (cons 'neomacs-smex-test-deploy 1)
              (list 'neomacs-smex-test-archive)))
       (choices '("neomacs-smex-test-deploy" "neomacs-smex-test-review")))
  (cl-progv
      '(extended-command-history command-history suggest-key-bindings
        smex-auto-update smex-initialized-p smex-history-length smex-data
        smex-cache smex-ido-cache smex-history)
      (list nil nil nil nil t 2 items items (smex-convert-for-ido items) nil)
    (unwind-protect
        (progn
        (fset 'neomacs-smex-test-build
              (lambda () (interactive) (push (list :build current-prefix-arg) events)))
        (fset 'neomacs-smex-test-review
              (lambda ()
                (interactive)
                (push (list :review current-prefix-arg this-command
                            real-this-command)
                      events)))
        (fset 'neomacs-smex-test-deploy
              (lambda ()
                (interactive)
                (push (list :deploy current-prefix-arg this-command
                            real-this-command)
                      events)))
        (fset 'neomacs-smex-test-archive
              (lambda () (interactive) (push (list :archive current-prefix-arg) events)))
        (cl-letf (((symbol-function 'smex-completing-read)
                   (lambda (_commands _initial-input) (pop choices))))
          (let ((current-prefix-arg '(4)))
            (smex))
          (let ((current-prefix-arg nil))
            (smex)))
        (smex-save-history)
        (list
         :events (nreverse events)
         :cache (mapcar #'car smex-cache)
         :items
         (neomacs-smex-test-items
          '(neomacs-smex-test-build neomacs-smex-test-review
            neomacs-smex-test-deploy neomacs-smex-test-archive))
         :history smex-history
         :extended-history extended-command-history
         :command-history command-history))
      (mapc (lambda (symbol) (when (fboundp symbol) (fmakunbound symbol)))
            '(neomacs-smex-test-build neomacs-smex-test-review
              neomacs-smex-test-deploy neomacs-smex-test-archive)))))
"###;
    let expected = expect![
        "OK (:events ((:deploy (4) neomacs-smex-test-deploy neomacs-smex-test-deploy) (:review nil neomacs-smex-test-review neomacs-smex-test-review)) :cache (neomacs-smex-test-review neomacs-smex-test-deploy neomacs-smex-test-build neomacs-smex-test-archive) :items ((neomacs-smex-test-build . 5) (neomacs-smex-test-review . 3) (neomacs-smex-test-deploy . 2) (neomacs-smex-test-archive)) :history (neomacs-smex-test-review neomacs-smex-test-deploy) :extended-history nil :command-history ((neomacs-smex-test-review) (neomacs-smex-test-deploy)))"
    ];
    ParityBatchCase::value(
        "selecting_commands_executes_with_prefix_and_promotes_real_usage",
        elisp_form,
        expected,
    )
}

fn usage_state_round_trips_through_the_configured_save_file() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (save-file (expand-file-name "smex-persisted-state" root))
       (choices '("neomacs-smex-test-deploy"
                  "neomacs-smex-test-review"
                  "neomacs-smex-test-build"))
       events)
  (cl-progv
      '(smex-save-file smex-history-length smex-cache smex-ido-cache
        smex-data smex-history smex-command-count smex-initialized-p
        smex-auto-update kill-emacs-hook ido-mode suggest-key-bindings
        extended-command-history command-history)
      (list save-file 3 nil nil nil nil 0 nil nil nil t nil nil nil)
    (unwind-protect
        (progn
        (fset 'neomacs-smex-test-deploy
              (lambda () (interactive) (push :deploy events)))
        (fset 'neomacs-smex-test-review
              (lambda () (interactive) (push :review events)))
        (fset 'neomacs-smex-test-build
              (lambda () (interactive) (push :build events)))
        (when (file-exists-p smex-save-file) (delete-file smex-save-file))
        (smex-initialize)
        (cl-letf (((symbol-function 'smex-completing-read)
                   (lambda (_commands _initial-input) (pop choices))))
          (smex)
          (smex)
          (smex))
        (run-hooks 'kill-emacs-hook)
        (let ((saved-text
               (with-temp-buffer
                 (insert-file-contents-literally smex-save-file)
                 (buffer-string))))
          (setq smex-history nil
                smex-data nil
                smex-cache nil
                smex-ido-cache nil
                smex-command-count 0
                smex-initialized-p nil
                kill-emacs-hook nil)
          (smex-initialize)
          (list :events (nreverse events)
                :history smex-history
                :data smex-data
                :cache
                (neomacs-smex-test-items
                 '(neomacs-smex-test-build neomacs-smex-test-review
                   neomacs-smex-test-deploy))
                :top
                (cl-remove-if-not
                 (lambda (command)
                   (memq command
                         '(neomacs-smex-test-build neomacs-smex-test-review
                           neomacs-smex-test-deploy)))
                 (mapcar #'car (cl-subseq smex-cache 0 3)))
                :text saved-text
                :readable (file-readable-p smex-save-file))))
      (mapc (lambda (symbol) (when (fboundp symbol) (fmakunbound symbol)))
            '(neomacs-smex-test-build neomacs-smex-test-review
              neomacs-smex-test-deploy))
      (when (file-exists-p smex-save-file) (delete-file smex-save-file)))))
"###;
    let expected = expect![[
        r#"OK (:events (:deploy :review :build) :history (neomacs-smex-test-build neomacs-smex-test-review neomacs-smex-test-deploy) :data ((neomacs-smex-test-deploy . 1) (neomacs-smex-test-review . 1) (neomacs-smex-test-build . 1)) :cache ((neomacs-smex-test-build . 1) (neomacs-smex-test-review . 1) (neomacs-smex-test-deploy . 1)) :top (neomacs-smex-test-build neomacs-smex-test-review neomacs-smex-test-deploy) :text "\n;; ----- smex-history -----\n(\n neomacs-smex-test-build\n neomacs-smex-test-review\n neomacs-smex-test-deploy\n)\n\n;; ----- smex-data -----\n(\n (neomacs-smex-test-deploy . 1)\n (neomacs-smex-test-review . 1)\n (neomacs-smex-test-build . 1)\n)\n" :readable t)"#
    ]];
    ParityBatchCase::value(
        "usage_state_round_trips_through_the_configured_save_file",
        elisp_form,
        expected,
    )
}

fn major_mode_workflow_distinguishes_local_map_and_loaded_feature_commands() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((offered nil)
       (library
        (expand-file-name "neomacs-smex-work-mode.el"
                          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (items
        (list (cons 'neomacs-smex-mode-test 2)
              (cons 'neomacs-smex-work-mode 1)
              (cons 'neomacs-smex-mode-build 4)
              (cons 'neomacs-smex-mode-deploy 2)
              (cons 'neomacs-smex-global-command 9)
              (list 'neomacs-smex-cache-tail))))
  (cl-progv
      '(suggest-key-bindings extended-command-history smex-initialized-p
        smex-data smex-cache smex-ido-cache smex-history-length)
      (list nil nil t items items (smex-convert-for-ido items) 2)
    (unwind-protect
        (progn
        (with-temp-file library
          (insert
           ";;; neomacs-smex-work-mode.el --- Smex parity fixture -*- lexical-binding: t; -*-\n"
           "(defvar neomacs-smex-mode-events nil)\n"
           "(defun neomacs-smex-mode-test ()\n"
           "  (interactive)\n"
           "  (push :test neomacs-smex-mode-events))\n"
           "(define-derived-mode neomacs-smex-work-mode fundamental-mode \"Smex-Work\")\n"
           "(provide 'neomacs-smex-work-mode)\n"))
        (load library nil t)
        (fset 'neomacs-smex-mode-build
              (lambda () (interactive) :build))
        (fset 'neomacs-smex-mode-deploy
              (lambda () (interactive) :deploy))
        (define-key neomacs-smex-work-mode-map
                    (kbd "C-c b") 'neomacs-smex-mode-build)
        (let ((prefix-map (make-sparse-keymap)))
          (define-key prefix-map (kbd "d") 'neomacs-smex-mode-deploy)
          (define-key prefix-map (kbd "m") "keyboard macro")
          (define-key neomacs-smex-work-mode-map (kbd "C-c p") prefix-map))
        (let ((local-map-commands
               (smex-extract-commands-from-keymap
                neomacs-smex-work-mode-map)))
          (with-temp-buffer
            (neomacs-smex-work-mode)
            (cl-letf (((symbol-function 'smex-completing-read)
                       (lambda (commands _initial-input)
                         (setq offered commands)
                         "neomacs-smex-mode-test")))
              (smex-major-mode-commands)))
          (list :local-map-commands local-map-commands
                :offered offered
                :events neomacs-smex-mode-events
                :cache (mapcar #'car smex-cache)
                :items
                (neomacs-smex-test-items
                 '(neomacs-smex-mode-test neomacs-smex-work-mode
                   neomacs-smex-mode-build neomacs-smex-mode-deploy
                   neomacs-smex-global-command)))))
      (when (featurep 'neomacs-smex-work-mode)
        (unload-feature 'neomacs-smex-work-mode t))
      (mapc (lambda (symbol) (when (fboundp symbol) (fmakunbound symbol)))
            '(neomacs-smex-mode-build neomacs-smex-mode-deploy))
      (when (file-exists-p library) (delete-file library)))))
"###;
    let expected = expect![[
        r#"OK (:local-map-commands nil :offered ("neomacs-smex-mode-test" "neomacs-smex-work-mode") :events (:test) :cache (neomacs-smex-mode-test neomacs-smex-work-mode neomacs-smex-mode-build neomacs-smex-mode-deploy neomacs-smex-global-command neomacs-smex-cache-tail) :items ((neomacs-smex-mode-test . 3) (neomacs-smex-work-mode . 1) (neomacs-smex-mode-build . 4) (neomacs-smex-mode-deploy . 2) (neomacs-smex-global-command . 9)))"#
    ]];
    ParityBatchCase::value(
        "major_mode_workflow_distinguishes_local_map_and_loaded_feature_commands",
        elisp_form,
        expected,
    )
}

fn command_set_detection_and_idle_maintenance_track_additions_and_removals() -> ParityBatchCase {
    let elisp_form = r###"
(let (timer-calls callbacks)
  (cl-progv
      '(smex-command-count smex-cache smex-ido-cache smex-data smex-history
        smex-history-length)
      '(0 nil nil nil nil 2)
    (unwind-protect
        (progn
        (smex-detect-new-commands)
        (let ((baseline smex-command-count))
          (cl-letf (((symbol-function 'run-with-idle-timer)
                     (lambda (idle repeat callback &rest arguments)
                       (push callback callbacks)
                       (push (list idle repeat (car callback)
                                   (length (cdr callback)) arguments)
                             timer-calls)
                       'neomacs-smex-test-timer)))
            (smex-auto-update)
            (smex-auto-update 5))
          (fset 'neomacs-smex-test-late-command
                (lambda () (interactive) :late))
          (funcall (car callbacks))
          (let ((after-add
                 (list (- smex-command-count baseline)
                       (and (assq 'neomacs-smex-test-late-command
                                  smex-cache)
                            t)
                       (and (member "neomacs-smex-test-late-command"
                                    smex-ido-cache)
                            t))))
            (fmakunbound 'neomacs-smex-test-late-command)
            (funcall (car callbacks))
            (list
             :add after-add
             :remove
             (list (- smex-command-count baseline)
                   (and (assq 'neomacs-smex-test-late-command smex-cache) t)
                   (and (member "neomacs-smex-test-late-command"
                                smex-ido-cache)
                        t))
             :timers (nreverse timer-calls)))))
      (when (fboundp 'neomacs-smex-test-late-command)
        (fmakunbound 'neomacs-smex-test-late-command)))))
"###;
    let expected = expect![
        "OK (:add (1 t t) :remove (0 nil nil) :timers ((60 t lambda 2 nil) (5 t lambda 2 nil)))"
    ];
    ParityBatchCase::value(
        "command_set_detection_and_idle_maintenance_track_additions_and_removals",
        elisp_form,
        expected,
    )
}

fn malformed_nonempty_state_file_stops_public_startup_with_an_exact_error() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (save-file (expand-file-name "smex-invalid-state" root)))
  (cl-progv
      '(smex-save-file smex-initialized-p smex-cache smex-ido-cache
        smex-data smex-history smex-command-count kill-emacs-hook ido-mode)
      (list save-file nil nil nil nil nil 0 (copy-sequence kill-emacs-hook) t)
    (unwind-protect
        (progn
          (with-temp-file smex-save-file (insert "(unfinished"))
          (smex))
      (when (file-exists-p smex-save-file) (delete-file smex-save-file)))))
"###;
    let expected = expect![[
        r#"ERR (error "Invalid data in smex-save-file ([ORACLE-SANDBOX]/smex-invalid-state). Can’t restore history.")"#
    ]];
    ParityBatchCase::signal(
        "malformed_nonempty_state_file_stops_public_startup_with_an_exact_error",
        elisp_form,
        expected,
    )
}

fn where_is_action_operates_on_the_selected_command_without_executing_it() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((key (kbd "C-c S"))
       (old-binding (lookup-key global-map key))
       (executions 0)
       (items (list (cons 'neomacs-smex-test-locate 2))))
  (cl-progv
      '(smex-custom-action smex-cache smex-data smex-ido-cache
        smex-initialized-p smex-auto-update)
      (list nil items items '("neomacs-smex-test-locate") t nil)
    (unwind-protect
        (progn
        (fset 'neomacs-smex-test-locate
              (lambda () (interactive) (setq executions (1+ executions))))
        (define-key global-map key 'neomacs-smex-test-locate)
        (cl-letf (((symbol-function 'ido-exit-minibuffer) (lambda () :exit)))
          (smex-where-is))
        (let ((armed (functionp smex-custom-action))
              where-is-output)
          (setq where-is-output
                (with-output-to-string
                  (cl-letf (((symbol-function 'smex-completing-read)
                             (lambda (_commands _initial-input)
                               "neomacs-smex-test-locate")))
                    (smex))))
          (list :armed armed
                :output where-is-output
                :executions executions
                :action-reset smex-custom-action
                :counter (cdr (assq 'neomacs-smex-test-locate smex-cache)))))
      (define-key global-map key old-binding)
      (when (fboundp 'neomacs-smex-test-locate)
        (fmakunbound 'neomacs-smex-test-locate)))))
"###;
    let expected = expect![[
        r#"OK (:armed t :output "neomacs-smex-test-locate is on C-c S" :executions 0 :action-reset nil :counter 2)"#
    ]];
    ParityBatchCase::value(
        "where_is_action_operates_on_the_selected_command_without_executing_it",
        elisp_form,
        expected,
    )
}

fn unbound_command_report_orders_real_usage_and_excludes_key_bound_commands() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((key (kbd "C-c U"))
       (old-binding (lookup-key global-map key))
       (report-buffer "*Smex: Unbound Commands*")
       (items
        (list (cons 'neomacs-smex-test-bound 4)
              (cons 'neomacs-smex-test-rare 2)
              (cons 'neomacs-smex-test-frequent 9))))
  (cl-progv '(smex-data) (list items)
    (unwind-protect
        (progn
        (fset 'neomacs-smex-test-bound (lambda () (interactive)))
        (fset 'neomacs-smex-test-rare (lambda () (interactive)))
        (fset 'neomacs-smex-test-frequent (lambda () (interactive)))
        (define-key global-map key 'neomacs-smex-test-bound)
        (save-window-excursion (smex-show-unbound-commands))
        (with-current-buffer report-buffer
          (list :contents (buffer-string)
                :read-only buffer-read-only
                :modified (buffer-modified-p)
                :usage-order (mapcar #'car smex-data))))
      (define-key global-map key old-binding)
      (when (get-buffer report-buffer) (kill-buffer report-buffer))
      (mapc (lambda (symbol) (when (fboundp symbol) (fmakunbound symbol)))
            '(neomacs-smex-test-bound neomacs-smex-test-rare
              neomacs-smex-test-frequent)))))
"###;
    let expected = expect![[
        r#"OK (:contents "\n;; ----- unbound-commands -----\n(\n (neomacs-smex-test-frequent . 9)\n (neomacs-smex-test-rare . 2)\n)\n" :read-only t :modified nil :usage-order (neomacs-smex-test-frequent neomacs-smex-test-bound neomacs-smex-test-rare))"#
    ]];
    ParityBatchCase::value(
        "unbound_command_report_orders_real_usage_and_excludes_key_bound_commands",
        elisp_form,
        expected,
    )
}

#[test]
fn smex_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(SMEX_MELPA_PIN, "smex.el")
            .expect("prepare revision-pinned Smex below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "smex-package-batch",
        "Smex",
        &[
            package_contract_exposes_the_m_x_workflow_and_user_policy(),
            initialization_discovers_commands_and_refreshes_after_the_command_set_changes(),
            selecting_commands_executes_with_prefix_and_promotes_real_usage(),
            usage_state_round_trips_through_the_configured_save_file(),
            major_mode_workflow_distinguishes_local_map_and_loaded_feature_commands(),
            command_set_detection_and_idle_maintenance_track_additions_and_removals(),
            malformed_nonempty_state_file_stops_public_startup_with_an_exact_error(),
            where_is_action_operates_on_the_selected_command_without_executing_it(),
            unbound_command_report_orders_real_usage_and_excludes_key_bound_commands(),
        ],
    );
}
