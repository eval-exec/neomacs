use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, RESTART_EMACS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const RESTART_EMACS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const RESTART_EMACS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'restart-emacs)

(defun neomacs-restart-emacs-test-capture (function)
  "Call FUNCTION and return stable success or signal data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-restart-emacs-test-directory-probe
  (proc-directory lsof-directory messages-directory scratch-directory
                    home-directory user-profile-directory fallback-directory)
  "Exercise startup-directory precedence with deterministic candidates."
  (let* ((messages-buffer (get-buffer-create "*Messages*"))
         (scratch-buffer (get-buffer-create "*scratch*"))
         (probe-buffer (generate-new-buffer " *restart-directory-probe*"))
         (messages-original-directory
          (with-current-buffer messages-buffer default-directory))
         (scratch-original-directory
          (with-current-buffer scratch-buffer default-directory))
         calls)
    (unwind-protect
        (progn
          (with-current-buffer messages-buffer
            (setq-local default-directory
                        (or messages-directory "/unused/messages/")))
          (with-current-buffer scratch-buffer
            (setq-local default-directory
                        (or scratch-directory "/unused/scratch/")))
          (cl-letf (((symbol-function 'restart-emacs--guess-startup-directory-using-proc)
                     (lambda ()
                       (push :proc calls)
                       proc-directory))
                    ((symbol-function 'restart-emacs--guess-startup-directory-using-lsof)
                     (lambda ()
                       (push :lsof calls)
                       lsof-directory))
                    ((symbol-function 'get-buffer)
                     (lambda (name)
                       (push (list :buffer name) calls)
                       (cond ((equal name "*Messages*")
                              (and messages-directory messages-buffer))
                             ((equal name "*scratch*")
                              (and scratch-directory scratch-buffer)))))
                    ((symbol-function 'getenv)
                     (lambda (name)
                       (push (list :environment name) calls)
                       (cond ((equal name "HOME") home-directory)
                             ((equal name "USERPROFILE") user-profile-directory)))))
            (with-current-buffer probe-buffer
              (setq-local default-directory fallback-directory)
              (list :directory (restart-emacs--guess-startup-directory)
                    :calls (nreverse calls)))))
      (with-current-buffer messages-buffer
        (setq-local default-directory messages-original-directory))
      (with-current-buffer scratch-buffer
        (setq-local default-directory scratch-original-directory))
      (when (buffer-live-p probe-buffer)
        (kill-buffer probe-buffer)))))
"####;

fn prefix_arguments_and_command_switches_preserve_restart_intent() -> ParityBatchCase {
    let elisp_form = r####"
(let (prompts restored)
  (cl-letf (((symbol-function 'read-string)
             (lambda (prompt)
               (push prompt prompts)
               "--eval (setq ready t) --debug-init"))
            ((symbol-function 'restart-emacs--restore-frames-using-desktop)
             (lambda (file) (push file restored))))
    (let ((command-line-args-left
           '("/state/team desktop" "--visit" "release-notes.org")))
      (restart-emacs-handle-command-line-args "--restart-emacs-desktop")
      (list
       :prefixes
       (list :none (restart-emacs--translate-prefix-to-args nil)
             :debug (restart-emacs--translate-prefix-to-args '(4))
             :quick (restart-emacs--translate-prefix-to-args '(16))
             :prompted (restart-emacs--translate-prefix-to-args '(64))
             :unsupported (restart-emacs--translate-prefix-to-args '(8)))
       :prompts (nreverse prompts)
       :registered-handler
       (cdr (assoc "--restart-emacs-desktop" command-switch-alist))
       :restored (nreverse restored)
       :remaining-command-line command-line-args-left))))
"####;
    let expected = expect![[
        r#"OK (:prefixes (:none nil :debug ("--debug-init") :quick ("-Q") :prompted ("--eval" "(setq" "ready" "t)" "--debug-init") :unsupported nil) :prompts ("Arguments to start Emacs with (separated by space): ") :registered-handler restart-emacs-handle-command-line-args :restored ("/state/team desktop") :remaining-command-line ("--visit" "release-notes.org"))"#
    ]];
    ParityBatchCase::value(
        "prefix_arguments_and_command_switches_preserve_restart_intent",
        elisp_form,
        expected,
    )
}

fn binary_resolution_and_launch_commands_quote_real_world_arguments() -> ParityBatchCase {
    let elisp_form = r####"
(let (launches)
  (cl-letf (((symbol-function 'restart-emacs--get-emacs-binary)
             (lambda () "/opt/Neo Macs/bin/neomacs"))
            ((symbol-function 'call-process)
             (lambda (&rest arguments)
               (push (cons :call-process arguments) launches)
               0))
            ((symbol-function 'suspend-emacs)
             (lambda (command)
               (push (list :suspend command) launches)
               nil))
            ((symbol-function 'w32-shell-execute)
             (lambda (&rest arguments)
               (push (cons :windows-shell arguments) launches)
               t)))
    (restart-emacs--start-gui-using-sh
     '("--debug-init" "--eval" "(message \"release ready\")"))
    (let ((server-name "team server"))
      (restart-emacs--daemon-using-sh
       '("--eval" "(setq deployment 'green)")))
    (restart-emacs--start-emacs-in-terminal
     '("--name" "Release Candidate"))
    (restart-emacs--start-gui-on-windows
     '("--name" "Release Candidate"))
    (restart-emacs--daemon-on-windows
     '("--debug-init" "release notes.org")))
    (list
     :binaries
     (list
      :unix
      (let ((invocation-name "neomacs")
            (invocation-directory "/opt/Neo Macs/bin/")
            (system-type 'gnu/linux))
        (restart-emacs--get-emacs-binary))
      :windows-runemacs
      (cl-letf (((symbol-function 'file-exists-p) (lambda (_file) t)))
        (let ((invocation-name "neomacs.exe")
              (invocation-directory "/opt/Neo Macs/bin/")
              (system-type 'windows-nt))
          (restart-emacs--get-emacs-binary)))
      :windows-fallback
      (cl-letf (((symbol-function 'file-exists-p) (lambda (_file) nil)))
        (let ((invocation-name "neomacs.exe")
              (invocation-directory "/opt/Neo Macs/bin/")
              (system-type 'windows-nt))
          (restart-emacs--get-emacs-binary))))
     :launches (nreverse launches)))
"####;
    let expected = expect![[
        r#"OK (:binaries (:unix "/opt/Neo Macs/bin/neomacs" :windows-runemacs "/opt/Neo Macs/bin/runemacs.exe" :windows-fallback "/opt/Neo Macs/bin/neomacs.exe") :launches ((:call-process "sh" nil 0 nil "-c" "/opt/Neo\\ Macs/bin/neomacs --debug-init --eval \\(message\\ \\\"release\\ ready\\\"\\) &") (:call-process "sh" nil 0 nil "-c" "/opt/Neo\\ Macs/bin/neomacs --daemon=team server --eval \\(setq\\ deployment\\ \\'green\\) &") (:suspend "fg ; /opt/Neo\\ Macs/bin/neomacs --name Release\\ Candidate -nw") (:windows-shell "open" "/opt/Neo Macs/bin/neomacs" "--name Release Candidate") (:windows-shell "open" "/opt/Neo Macs/bin/neomacs" "--daemon=server --debug-init release notes.org")))"#
    ]];
    ParityBatchCase::value(
        "binary_resolution_and_launch_commands_quote_real_world_arguments",
        elisp_form,
        expected,
    )
}

fn launch_strategy_selects_daemon_gui_terminal_and_platform_backends() -> ParityBatchCase {
    let elisp_form = r####"
(let (daemon graphic backends)
  (cl-letf (((symbol-function 'daemonp) (lambda () daemon))
            ((symbol-function 'display-graphic-p) (lambda (&optional _frame) graphic))
            ((symbol-function 'restart-emacs--daemon-using-sh)
             (lambda (args)
               (push (list :daemon-sh (copy-sequence args)) backends)
               :started))
            ((symbol-function 'restart-emacs--daemon-on-windows)
             (lambda (args)
               (push (list :daemon-windows (copy-sequence args)) backends)
               :started))
            ((symbol-function 'restart-emacs--start-gui-using-sh)
             (lambda (args)
               (push (list :gui-sh (copy-sequence args)) backends)
               :started))
            ((symbol-function 'restart-emacs--start-gui-on-windows)
             (lambda (args)
               (push (list :gui-windows (copy-sequence args)) backends)
               :started))
            ((symbol-function 'restart-emacs--start-emacs-in-terminal)
             (lambda (args)
               (push (list :terminal (copy-sequence args)) backends)
               :started)))
    (let ((scenarios '((daemon-linux gnu/linux t nil)
                       (daemon-windows windows-nt t t)
                       (gui-linux gnu/linux nil t)
                       (gui-windows windows-nt nil t)
                       (terminal-linux gnu/linux nil nil)
                       (terminal-windows windows-nt nil nil)))
          results)
      (dolist (scenario scenarios)
        (setq daemon (nth 2 scenario)
              graphic (nth 3 scenario))
        (let ((system-type (nth 1 scenario)))
          (push
           (list (car scenario)
                 (neomacs-restart-emacs-test-capture
                  (lambda ()
                    (restart-emacs--launch-other-emacs
                     '("--name" "release")))))
           results)))
      (list :results (nreverse results)
            :backends (nreverse backends)))))
"####;
    let expected = expect![[
        r#"OK (:results ((daemon-linux (:ok :started)) (daemon-windows (:ok :started)) (gui-linux (:ok :started)) (gui-windows (:ok :started)) (terminal-linux (:ok :started)) (terminal-windows (:error user-error :data ("Cannot restart Emacs running in a windows terminal") :message "Cannot restart Emacs running in a windows terminal"))) :backends ((:daemon-sh ("--name" "release")) (:daemon-windows ("--name" "release")) (:gui-sh ("--name" "release")) (:gui-windows ("--name" "release")) (:terminal ("--name" "release"))))"#
    ]];
    ParityBatchCase::value(
        "launch_strategy_selects_daemon_gui_terminal_and_platform_backends",
        elisp_form,
        expected,
    )
}

fn restart_capability_checks_protect_unsupported_and_tty_sessions() -> ParityBatchCase {
    let elisp_form = r####"
(let (daemon graphic frameset tty answer prompts)
  (cl-letf (((symbol-function 'daemonp) (lambda () daemon))
            ((symbol-function 'display-graphic-p) (lambda (&optional _frame) graphic))
            ((symbol-function 'locate-library)
             (lambda (library &rest _ignored)
               (and frameset (equal library "frameset") "/lisp/frameset.el")))
            ((symbol-function 'frame-list) (lambda () '(main-frame tty-frame)))
            ((symbol-function 'frame-parameter)
             (lambda (frame parameter)
               (and tty (eq frame 'tty-frame) (eq parameter 'tty) "/dev/pts/9")))
            ((symbol-function 'yes-or-no-p)
             (lambda (prompt)
               (push prompt prompts)
               answer)))
    (let ((scenarios '((ordinary-terminal gnu/linux nil nil t nil nil)
                       (windows-terminal windows-nt nil nil t nil nil)
                       (old-daemon gnu/linux t nil nil nil nil)
                       (tty-daemon-decline gnu/linux t nil t t nil)
                       (tty-daemon-accept gnu/linux t nil t t t)
                       (tty-daemon-configured gnu/linux t nil t t nil)))
          results)
      (dolist (scenario scenarios)
        (setq daemon (nth 2 scenario)
              graphic (nth 3 scenario)
              frameset (nth 4 scenario)
              tty (nth 5 scenario)
              answer (nth 6 scenario)
              prompts nil)
        (let ((system-type (nth 1 scenario))
              (restart-emacs-daemon-with-tty-frames-p
               (eq (car scenario) 'tty-daemon-configured)))
          (push
           (list (car scenario)
                 :outcome
                 (neomacs-restart-emacs-test-capture
                  #'restart-emacs--ensure-can-restart)
                 :prompts (nreverse prompts))
           results)))
      (nreverse results))))
"####;
    let expected = expect![[
        r#"OK ((ordinary-terminal :outcome (:ok nil) :prompts nil) (windows-terminal :outcome (:error user-error :data ("Cannot restart Emacs running in terminal on system of type ‘windows-nt’") :message "Cannot restart Emacs running in terminal on system of type ‘windows-nt’") :prompts nil) (old-daemon :outcome (:error user-error :data ("Cannot restart Emacs daemon on versions before 24.4") :message "Cannot restart Emacs daemon on versions before 24.4") :prompts nil) (tty-daemon-decline :outcome (:error user-error :data ("Current Emacs daemon has tty frames, aborting ‘restart-emacs’.\nSet ‘restart-emacs-with-tty-frames-p’ to non-nil to restart Emacs irrespective of tty frames") :message "Current Emacs daemon has tty frames, aborting ‘restart-emacs’.\nSet ‘restart-emacs-with-tty-frames-p’ to non-nil to restart Emacs irrespective of tty frames") :prompts ("Current Emacs daemon has tty frames, `restart-emacs' cannot restore them, continue anyway? ")) (tty-daemon-accept :outcome (:ok nil) :prompts ("Current Emacs daemon has tty frames, `restart-emacs' cannot restore them, continue anyway? ")) (tty-daemon-configured :outcome (:ok nil) :prompts nil))"#
    ]];
    ParityBatchCase::value(
        "restart_capability_checks_protect_unsupported_and_tty_sessions",
        elisp_form,
        expected,
    )
}

fn startup_directory_fallbacks_keep_new_sessions_in_the_original_project() -> ParityBatchCase {
    let elisp_form = r####"
(list
 (neomacs-restart-emacs-test-directory-probe
  "/launch/from-proc/" "/launch/from-lsof/" "/project/messages/"
  "/project/scratch/" "/home/builder" "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil "/launch/from-lsof/" "/project/messages/" "/project/scratch/"
  "/home/builder" "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil nil "/project/messages/" "/project/scratch/"
  "/home/builder" "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil nil nil "/project/scratch/" "/home/builder"
  "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil nil nil nil "/home/builder" "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil nil nil nil nil "C:/Users/builder" "/fallback/")
 (neomacs-restart-emacs-test-directory-probe
  nil nil nil nil nil nil "/fallback/current-project/"))
"####;
    let expected = expect![[
        r#"OK ((:directory "/launch/from-proc/" :calls (:proc)) (:directory "/launch/from-lsof/" :calls (:proc :lsof)) (:directory "/project/messages/" :calls (:proc :lsof (:buffer "*Messages*"))) (:directory "/project/scratch/" :calls (:proc :lsof (:buffer "*Messages*") (:buffer "*scratch*"))) (:directory "/home/builder" :calls (:proc :lsof (:buffer "*Messages*") (:buffer "*scratch*") (:environment "HOME"))) (:directory "C:/Users/builder" :calls (:proc :lsof (:buffer "*Messages*") (:buffer "*scratch*") (:environment "HOME") (:environment "USERPROFILE"))) (:directory "/fallback/current-project/" :calls (:proc :lsof (:buffer "*Messages*") (:buffer "*scratch*") (:environment "HOME") (:environment "USERPROFILE"))))"#
    ]];
    ParityBatchCase::value(
        "startup_directory_fallbacks_keep_new_sessions_in_the_original_project",
        elisp_form,
        expected,
    )
}

fn restart_and_start_new_build_ordered_transactions_without_leaking_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(let (events)
  (cl-letf (((symbol-function 'restart-emacs--ensure-can-restart)
             (lambda () (push :capability-checked events)))
            ((symbol-function 'restart-emacs--guess-startup-directory)
             (lambda ()
               (push :startup-directory-guessed events)
               "/workspace/release/"))
            ((symbol-function 'restart-emacs--frame-restore-args)
             (lambda ()
               (push :frame-state-saved events)
               '("--restart-emacs-desktop" "/state/team desktop")))
            ((symbol-function 'restart-emacs--launch-other-emacs)
             (lambda (arguments)
               (push (list :launched arguments :directory default-directory) events)
               :launched))
            ((symbol-function 'save-buffers-kill-emacs)
             (lambda (&rest arguments)
               (push (list :save-requested arguments
                           :directory default-directory
                           :hook-count (length kill-emacs-hook))
                     events)
               (run-hooks 'kill-emacs-hook)
               :save-requested)))
    (let ((kill-emacs-hook
           (list (lambda () (push :existing-kill-hook events)))))
      (let ((restart-result
             (restart-emacs
              '("--debug-init" "--eval" "(setq release-ready t)")))
            restart-events)
        (setq restart-events (nreverse events)
              events nil)
        (let ((start-new-result (restart-emacs-start-new-emacs '("-Q"))))
          (list :restart-result restart-result
                :restart-events restart-events
                :outer-hook-count (length kill-emacs-hook)
                :start-new-result start-new-result
                :start-new-events (nreverse events)
                :inhibit-after restart-emacs--inhibit-kill-p))))))
"####;
    let expected = expect![[
        r#"OK (:restart-result :save-requested :restart-events (:capability-checked :startup-directory-guessed :frame-state-saved (:save-requested nil :directory "/workspace/release/" :hook-count 2) :existing-kill-hook (:launched ("--debug-init" "--eval" "(setq release-ready t)" "--restart-emacs-desktop" "/state/team desktop") :directory "/workspace/release/")) :outer-hook-count 1 :start-new-result :launched :start-new-events (:capability-checked :startup-directory-guessed (:launched ("-Q") :directory "/workspace/release/")) :inhibit-after nil)"#
    ]];
    ParityBatchCase::value(
        "restart_and_start_new_build_ordered_transactions_without_leaking_hooks",
        elisp_form,
        expected,
    )
}

fn frame_restore_policy_and_desktop_handoff_preserve_session_state() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((frame-policy
      (frameset daemon restore desktop-active)
      (let (saved)
        (cl-letf (((symbol-function 'locate-library)
                   (lambda (library &rest _ignored)
                     (and frameset (equal library "frameset") "/lisp/frameset.el")))
                  ((symbol-function 'daemonp) (lambda () daemon))
                  ((symbol-function 'restart-emacs--save-frames-using-desktop)
                   (lambda ()
                     (push "/state/restart-desktop" saved)
                     "/state/restart-desktop")))
          (let ((restart-emacs-restore-frames restore)
                (desktop-save-mode desktop-active))
            (list :args (restart-emacs--frame-restore-args)
                  :saved (nreverse saved)))))))
  (let ((original-color (symbol-function 'display-color-p))
        (original-graphic (symbol-function 'display-graphic-p))
        events)
    (cl-letf (((symbol-function 'daemonp) (lambda () t))
              ((symbol-function 'desktop-read)
               (lambda (directory &rest _ignored)
                 (push (list :desktop-read directory
                             :dirname desktop-dirname
                             :base desktop-base-file-name
                             :lock desktop-base-lock-name
                             :color (display-color-p)
                             :graphic (display-graphic-p))
                       events)
                 t))
              ((symbol-function 'desktop-release-lock)
               (lambda (directory)
                 (push (list :release-lock directory) events)))
              ((symbol-function 'delete-file)
               (lambda (file &optional _trash)
                 (push (list :delete file) events))))
      (restart-emacs--restore-frames-using-desktop "/state/restart-desktop")
      (list
       :policies
       (list
        :no-frameset (frame-policy nil nil t nil)
        :daemon (frame-policy t t nil t)
        :configured (frame-policy t nil t nil)
        :desktop-owner (frame-policy t nil t t)
        :disabled (frame-policy t nil nil nil))
       :restore-events (nreverse events)
       :display-functions-restored
       (list (eq original-color (symbol-function 'display-color-p))
             (eq original-graphic (symbol-function 'display-graphic-p)))))))
"####;
    let expected = expect![[
        r#"OK (:policies (:no-frameset (:args nil :saved nil) :daemon (:args ("--restart-emacs-desktop" "/state/restart-desktop") :saved ("/state/restart-desktop")) :configured (:args ("--restart-emacs-desktop" "/state/restart-desktop") :saved ("/state/restart-desktop")) :desktop-owner (:args nil :saved nil) :disabled (:args nil :saved nil)) :restore-events ((:desktop-read "/state/" :dirname "/state/" :base "restart-desktop" :lock "restart-desktop.lock" :color t :graphic t) (:release-lock "/state/") (:delete "/state/restart-desktop") (:delete "/state/restart-desktop.lock")) :display-functions-restored (t t))"#
    ]];
    ParityBatchCase::value(
        "frame_restore_policy_and_desktop_handoff_preserve_session_state",
        elisp_form,
        expected,
    )
}

fn daemon_tty_notifications_write_reconnect_commands_with_safe_quoting() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((directory (file-name-as-directory (getenv "TMPDIR")))
       (tty-file (expand-file-name "restart-emacs-tty-notification" directory))
       (server-use-tcp nil)
       (server-socket-dir "/run/user/1000/neomacs sockets/")
       (server-name "release server")
       (invocation-directory "/opt/Neo Macs/bin/")
       with-file without-file)
  (unwind-protect
      (progn
        (restart-emacs--notify-connection-instructions
         tty-file "/workspace/Release Notes.org")
        (setq with-file
              (with-temp-buffer
                (insert-file-contents tty-file)
                (buffer-string)))
        (restart-emacs--notify-connection-instructions tty-file nil)
        (setq without-file
              (with-temp-buffer
                (insert-file-contents tty-file)
                (buffer-string)))
        (list :with-file with-file :without-file without-file))
    (when (file-exists-p tty-file)
      (delete-file tty-file))))
"####;
    let expected = expect![[
        r#"OK (:with-file "Emacs daemon restarted! Use '/opt/Neo\\ Macs/bin/emacsclient -nw -s /run/user/1000/neomacs\\ sockets/release\\ server /workspace/Release\\ Notes.org' to reconnect to it" :without-file "Emacs daemon restarted! Use '/opt/Neo\\ Macs/bin/emacsclient -nw -s /run/user/1000/neomacs\\ sockets/release\\ server' to reconnect to it")"#
    ]];
    ParityBatchCase::value(
        "daemon_tty_notifications_write_reconnect_commands_with_safe_quoting",
        elisp_form,
        expected,
    )
}

#[test]
fn restart_emacs_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(RESTART_EMACS_MELPA_PIN, "restart-emacs.el")
            .expect("prepare revision-pinned Restart Emacs source below ./tmp")
            .with_timeout(RESTART_EMACS_TEST_TIMEOUT)
            .with_prelude(RESTART_EMACS_TEST_PRELUDE),
        "restart-emacs-package-batch",
        "Restart Emacs",
        &[
            prefix_arguments_and_command_switches_preserve_restart_intent(),
            binary_resolution_and_launch_commands_quote_real_world_arguments(),
            launch_strategy_selects_daemon_gui_terminal_and_platform_backends(),
            restart_capability_checks_protect_unsupported_and_tty_sessions(),
            startup_directory_fallbacks_keep_new_sessions_in_the_original_project(),
            restart_and_start_new_build_ordered_transactions_without_leaking_hooks(),
            frame_restore_policy_and_desktop_handoff_preserve_session_state(),
            daemon_tty_notifications_write_reconnect_commands_with_safe_quoting(),
        ],
    );
}
