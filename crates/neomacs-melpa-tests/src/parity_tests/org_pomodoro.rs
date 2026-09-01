use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_POMODORO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'org-pomodoro)

(defun neomacs-org-pomodoro-test-mode-line ()
  "Return stable mode-line text and its phase face."
  (when (consp org-pomodoro-mode-line)
    (let ((formatted (mapconcat
                      (lambda (part)
                        (if (stringp part) part (format "%s" part)))
                      org-pomodoro-mode-line "")))
      (list :text (substring-no-properties formatted)
            :face (get-text-property 0 'face (nth 1 org-pomodoro-mode-line))))))

(defun neomacs-org-pomodoro-test-state (now)
  "Return stable timer state relative to NOW."
  (list :state org-pomodoro-state
        :active (org-pomodoro-active-p)
        :count org-pomodoro-count
        :remaining (and org-pomodoro-end-time
                        (truncate (float-time
                                   (time-subtract org-pomodoro-end-time now))))
        :timer (if (consp org-pomodoro-timer)
                   (car org-pomodoro-timer)
                 org-pomodoro-timer)
        :mode-line (neomacs-org-pomodoro-test-mode-line)
        :registered (and (memq 'org-pomodoro-mode-line global-mode-string) t)))

(defun neomacs-org-pomodoro-test-error (function)
  "Return FUNCTION's value or stable error details."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn package_contract_exposes_the_task_timer_and_user_policy() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'org-pomodoro package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'org-pomodoro) t))
   :interactive
   (mapcar #'commandp '(org-pomodoro org-pomodoro-extend-last-clock))
   :controllers
   (mapcar #'fboundp '(org-pomodoro-start org-pomodoro-tick
                       org-pomodoro-kill org-pomodoro-reset))
   :policy
   (list :pomodoro org-pomodoro-length
         :short-break org-pomodoro-short-break-length
         :long-break org-pomodoro-long-break-length
         :long-frequency org-pomodoro-long-break-frequency
         :expiry org-pomodoro-expiry-time
         :manual org-pomodoro-manual-break
         :ask-on-kill org-pomodoro-ask-upon-killing
         :clock-break org-pomodoro-clock-break
         :keep-killed org-pomodoro-keep-killed-pomodoro-time
         :time-format org-pomodoro-time-format)
   :hooks
   (mapcar #'boundp
           '(org-pomodoro-started-hook org-pomodoro-finished-hook
             org-pomodoro-overtime-hook org-pomodoro-killed-hook
             org-pomodoro-break-finished-hook
             org-pomodoro-short-break-finished-hook
             org-pomodoro-long-break-finished-hook
             org-pomodoro-tick-hook))))
"###;
    let expected = expect![[
        r#"OK (:package (:name org-pomodoro :version "20220318.1618" :requirements ((alert (0 5 10)) (cl-lib (0 5))) :feature t) :interactive (t t) :controllers (t t t t) :policy (:pomodoro 25 :short-break 5 :long-break 20 :long-frequency 4 :expiry 120 :manual nil :ask-on-kill t :clock-break nil :keep-killed nil :time-format "%.2m:%.2s") :hooks (t t t t t t t t))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_the_task_timer_and_user_policy",
        elisp_form,
        expected,
    )
}

fn starting_and_resetting_a_focus_session_manage_timer_and_mode_line_lifecycle() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((events nil)
       (now (seconds-to-time 1800000000))
       (org-pomodoro-length 25)
       (org-pomodoro-state :none)
       (org-pomodoro-count 2)
       (org-pomodoro-timer :old-timer)
       (org-pomodoro-end-time nil)
       (org-pomodoro-mode-line nil)
       (global-mode-string nil)
       (org-pomodoro-started-hook
        (list (lambda () (push 'started events)))))
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'run-with-timer)
             (lambda (&rest arguments)
               (push (cons 'scheduled arguments) events)
               (cons :timer arguments)))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push (list 'cancelled timer) events)))
            ((symbol-function 'org-pomodoro-maybe-play-sound)
             (lambda (type) (push (list 'sound type) events)))
            ((symbol-function 'force-mode-line-update)
             (lambda (&optional all) (push (list 'mode-line all) events)))
            ((symbol-function 'org-agenda-maybe-redo)
             (lambda () (push 'agenda-redo events))))
    (org-pomodoro-start)
    (let ((started-state (neomacs-org-pomodoro-test-state now))
          (registered (copy-sequence global-mode-string)))
      (org-pomodoro-reset)
      (list :started started-state
            :registered registered
            :reset (neomacs-org-pomodoro-test-state now)
            :events (nreverse events)))))
"###;
    let expected = expect![[
        r#"OK (:started (:state :pomodoro :active t :count 2 :remaining 1500 :timer :timer :mode-line (:text "[Pomodoro~25:00] " :face org-pomodoro-mode-line) :registered t) :registered ("" org-pomodoro-mode-line) :reset (:state :none :active nil :count 2 :remaining nil :timer :timer :mode-line nil :registered t) :events ((cancelled :old-timer) (scheduled . #1=(t 1 org-pomodoro-tick)) (sound :start) started (mode-line t) agenda-redo (cancelled (:timer . #1#)) (mode-line t) agenda-redo))"#
    ]];
    ParityBatchCase::value(
        "starting_and_resetting_a_focus_session_manage_timer_and_mode_line_lifecycle",
        elisp_form,
        expected,
    )
}

fn a_real_org_task_clocks_in_for_the_focus_session_and_out_for_its_break() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((events nil)
       (now (seconds-to-time 1800000000))
       (process-environment (cons "TZ=UTC0" process-environment))
       (org-pomodoro-length 25)
       (org-pomodoro-long-break-frequency 4)
       (org-pomodoro-clock-break nil)
       (org-pomodoro-play-sounds nil)
       (org-pomodoro-state :none)
       (org-pomodoro-count 0)
       (org-pomodoro-timer nil)
       (org-pomodoro-end-time nil)
       (org-pomodoro-last-clock-in nil)
       (org-pomodoro-mode-line "")
       (global-mode-string nil)
       (org-clock-persist nil)
       (org-log-done nil)
       events)
  (unwind-protect
      (with-temp-buffer
        (org-mode)
        (insert "* TODO Ship deterministic timer\n")
        (goto-char (point-min))
        (cl-letf (((symbol-function 'current-time) (lambda () now))
                  ((symbol-function 'run-with-timer)
                   (lambda (&rest arguments) (cons :timer arguments)))
                  ((symbol-function 'cancel-timer) (lambda (&rest _)))
                  ((symbol-function 'org-pomodoro-maybe-play-sound)
                   (lambda (type) (push (list 'sound type) events)))
                  ((symbol-function 'org-pomodoro-notify)
                   (lambda (title message)
                     (push (list 'notify title message) events)))
                  ((symbol-function 'force-mode-line-update) (lambda (&rest _)))
                  ((symbol-function 'org-agenda-maybe-redo) (lambda (&rest _))))
          (org-pomodoro)
          (let ((clocked
                 (list :clocking (and (org-clocking-p) t)
                       :heading org-clock-heading
                       :start-time (equal org-clock-start-time now)
                       :marker-buffer (eq (marker-buffer org-clock-marker)
                                          (current-buffer))
                       :state (neomacs-org-pomodoro-test-state now))))
            (setq now (time-add now (* 25 60)))
            (org-pomodoro-finished)
            (list :clocked clocked
                  :finished
                  (list :clocking (and (org-clocking-p) t)
                        :state (neomacs-org-pomodoro-test-state now)
                        :text (buffer-substring-no-properties
                               (point-min) (point-max)))
                  :events (nreverse events)))))
    (when (org-clocking-p)
      (org-clock-cancel))))
"###;
    let expected = expect![[
        r#"OK (:clocked (:clocking t :heading "Ship deterministic timer" :start-time t :marker-buffer t :state (:state :pomodoro :active t :count 0 :remaining 1500 :timer :timer :mode-line (:text "[Pomodoro~25:00] " :face org-pomodoro-mode-line) :registered t)) :finished (:clocking nil :state (:state :short-break :active t :count 1 :remaining 300 :timer :timer :mode-line (:text "[Short Break~05:00] " :face org-pomodoro-mode-line-break) :registered t) :text "* TODO Ship deterministic timer\n:LOGBOOK:\nCLOCK: [2027-01-15 Fri 08:00]--[2027-01-15 Fri 08:25] =>  0:25\n:END:\n") :events ((sound :start) (sound :pomodoro) (notify "Pomodoro completed!" "Time for a break.")))"#
    ]];
    ParityBatchCase::value(
        "a_real_org_task_clocks_in_for_the_focus_session_and_out_for_its_break",
        elisp_form,
        expected,
    )
}

fn an_expired_fourth_focus_tick_enters_a_long_break_and_runs_operational_hooks() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((events nil)
       (now (seconds-to-time 1800000000))
       (org-pomodoro-state :pomodoro)
       (org-pomodoro-count 3)
       (org-pomodoro-end-time (time-subtract now 1))
       (org-pomodoro-timer :focus-timer)
       (org-pomodoro-clock-break t)
       (org-pomodoro-manual-break nil)
       (org-pomodoro-long-break-frequency 4)
       (org-pomodoro-long-break-length 20)
       (org-pomodoro-ticking-sound-p t)
       (org-pomodoro-ticking-sound-states '(:pomodoro :long-break))
       (org-pomodoro-ticking-frequency 60)
       (org-pomodoro-finished-hook
        (list (lambda () (push 'focus-finished events))))
       (org-pomodoro-tick-hook
        (list (lambda () (push 'tick-hook events))))
       (global-mode-string nil))
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'run-with-timer)
             (lambda (&rest arguments)
               (push (cons 'scheduled arguments) events)
               (cons :timer arguments)))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push (list 'cancelled timer) events)))
            ((symbol-function 'org-pomodoro-maybe-play-sound)
             (lambda (type) (push (list 'sound type) events)))
            ((symbol-function 'org-pomodoro-notify)
             (lambda (title message)
               (push (list 'notify title message) events)))
            ((symbol-function 'force-mode-line-update) (lambda (&rest _)))
            ((symbol-function 'org-agenda-maybe-redo) (lambda (&rest _))))
    (org-pomodoro-tick)
    (list :state (neomacs-org-pomodoro-test-state now)
          :events (nreverse events))))
"###;
    let expected = expect![[
        r#"OK (:state (:state :long-break :active t :count 4 :remaining 1200 :timer :timer :mode-line (:text "[Long Break~20:00] " :face org-pomodoro-mode-line-break) :registered t) :events ((sound :pomodoro) (cancelled :focus-timer) (scheduled t 1 org-pomodoro-tick) (notify "Pomodoro completed!" "Time for a break.") focus-finished tick-hook (sound :tick)))"#
    ]];
    ParityBatchCase::value(
        "an_expired_fourth_focus_tick_enters_a_long_break_and_runs_operational_hooks",
        elisp_form,
        expected,
    )
}

fn manual_overtime_waits_for_the_user_before_starting_the_next_break() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((events nil)
       (now (seconds-to-time 1800000000))
       (org-pomodoro-state :pomodoro)
       (org-pomodoro-count 1)
       (org-pomodoro-end-time (time-subtract now 1))
       (org-pomodoro-timer :focus-timer)
       (org-pomodoro-clock-break t)
       (org-pomodoro-manual-break t)
       (org-pomodoro-long-break-frequency 4)
       (org-pomodoro-last-clock-in nil)
       (org-pomodoro-overtime-hook
        (list (lambda () (push 'overtime-hook events))))
       (org-pomodoro-finished-hook
        (list (lambda () (push 'finished-hook events))))
       (global-mode-string nil))
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'run-with-timer)
             (lambda (&rest arguments) (cons :timer arguments)))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push (list 'cancelled timer) events)))
            ((symbol-function 'org-pomodoro-maybe-play-sound)
             (lambda (type) (push (list 'sound type) events)))
            ((symbol-function 'org-pomodoro-notify)
             (lambda (title message)
               (push (list 'notify title message) events)))
            ((symbol-function 'force-mode-line-update) (lambda (&rest _)))
            ((symbol-function 'org-agenda-maybe-redo) (lambda (&rest _))))
    (org-pomodoro-tick)
    (let ((overtime (neomacs-org-pomodoro-test-state now)))
      (org-pomodoro)
      (list :overtime overtime
            :after-command (neomacs-org-pomodoro-test-state now)
            :last-clock-in (equal org-pomodoro-last-clock-in now)
            :events (nreverse events)))))
"###;
    let expected = expect![[
        r#"OK (:overtime (:state :overtime :active t :count 1 :remaining 0 :timer :timer :mode-line (:text "[+00:00] " :face org-pomodoro-mode-line-overtime) :registered t) :after-command (:state :short-break :active t :count 2 :remaining 300 :timer :timer :mode-line (:text "[Short Break~05:00] " :face org-pomodoro-mode-line-break) :registered t) :last-clock-in t :events ((sound :overtime) (notify "Pomodoro completed. Now on overtime!" "Start break by calling ‘org-pomodoro’") (cancelled :focus-timer) overtime-hook (sound :pomodoro) (cancelled (:timer t 1 org-pomodoro-tick)) (notify "Pomodoro completed!" "Time for a break.") finished-hook))"#
    ]];
    ParityBatchCase::value(
        "manual_overtime_waits_for_the_user_before_starting_the_next_break",
        elisp_form,
        expected,
    )
}

fn stopping_a_running_session_honors_confirmation_and_clock_retention_policy() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((events nil)
       (now (seconds-to-time 1800000000))
       (org-pomodoro-state :pomodoro)
       (org-pomodoro-count 2)
       (org-pomodoro-end-time (time-add now 600))
       (org-pomodoro-timer :focus-timer)
       (org-pomodoro-last-clock-in nil)
       (org-pomodoro-ask-upon-killing t)
       (org-pomodoro-keep-killed-pomodoro-time nil)
       (org-pomodoro-mode-line "")
       (global-mode-string nil)
       (org-pomodoro-killed-hook
        (list (lambda () (push 'killed-hook events))))
       (answers '(nil t)))
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'y-or-n-p)
             (lambda (prompt)
               (let ((answer (pop answers)))
                 (push (list 'confirm prompt answer) events)
                 answer)))
            ((symbol-function 'message)
             (lambda (format-string &rest arguments)
               (push (list 'message (apply #'format format-string arguments)) events)))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push (list 'cancelled timer) events)))
            ((symbol-function 'org-pomodoro-notify)
             (lambda (title message)
               (push (list 'notify title message) events)))
            ((symbol-function 'org-pomodoro-maybe-play-sound)
             (lambda (type) (push (list 'sound type) events)))
            ((symbol-function 'org-clocking-p) (lambda () t))
            ((symbol-function 'org-clock-cancel)
             (lambda () (push 'clock-cancel events)))
            ((symbol-function 'org-clock-out)
             (lambda (&rest arguments)
               (push (cons 'clock-out arguments) events)))
            ((symbol-function 'force-mode-line-update) (lambda (&rest _)))
            ((symbol-function 'org-agenda-maybe-redo) (lambda (&rest _))))
    (org-pomodoro)
    (let ((declined (neomacs-org-pomodoro-test-state now)))
      (org-pomodoro)
      (let ((cancelled (neomacs-org-pomodoro-test-state now)))
        (setq org-pomodoro-state :pomodoro
              org-pomodoro-end-time (time-add now 300)
              org-pomodoro-timer :second-timer
              org-pomodoro-ask-upon-killing nil
              org-pomodoro-keep-killed-pomodoro-time t)
        (org-pomodoro)
        (list :declined declined
              :cancelled cancelled
              :kept (neomacs-org-pomodoro-test-state now)
              :events (nreverse events))))))
"###;
    let expected = expect![[
        r#"OK (:declined (:state :pomodoro :active t :count 2 :remaining 600 :timer :focus-timer :mode-line nil :registered nil) :cancelled (:state :none :active nil :count 2 :remaining nil :timer :focus-timer :mode-line nil :registered nil) :kept (:state :none :active nil :count 2 :remaining nil :timer :second-timer :mode-line nil :registered nil) :events ((confirm "There is already a running timer.  Would you like to stop it? " nil) (message "Alright, keep up the good work!") (confirm "There is already a running timer.  Would you like to stop it? " t) (cancelled :focus-timer) (notify "Pomodoro killed." "One does not simply kill a pomodoro!") (sound :killed) clock-cancel killed-hook (cancelled :second-timer) (notify "Pomodoro killed." "One does not simply kill a pomodoro!") (sound :killed) (clock-out nil t) killed-hook))"#
    ]];
    ParityBatchCase::value(
        "stopping_a_running_session_honors_confirmation_and_clock_retention_policy",
        elisp_form,
        expected,
    )
}

fn short_and_long_break_completion_clock_out_reset_and_run_hooks_in_order() -> ParityBatchCase {
    let elisp_form = r###"
(let ((now (seconds-to-time 1800000000))
      (org-pomodoro-clock-break t)
      events)
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push (list 'cancelled timer) events)))
            ((symbol-function 'org-clock-out)
             (lambda (&rest arguments)
               (push (cons 'clock-out arguments) events)))
            ((symbol-function 'org-pomodoro-notify)
             (lambda (title message)
               (push (list 'notify title message) events)))
            ((symbol-function 'org-pomodoro-maybe-play-sound)
             (lambda (type) (push (list 'sound type) events)))
            ((symbol-function 'force-mode-line-update) (lambda (&rest _)))
            ((symbol-function 'org-agenda-maybe-redo) (lambda (&rest _))))
    (let ((org-pomodoro-state :short-break)
          (org-pomodoro-end-time (time-add now 15))
          (org-pomodoro-timer :short-timer)
          (org-pomodoro-break-finished-hook
           (list (lambda () (push 'break-hook events))))
          (org-pomodoro-short-break-finished-hook
           (list (lambda () (push 'short-hook events)))))
      (org-pomodoro-short-break-finished)
      (push (list 'short-state (neomacs-org-pomodoro-test-state now)) events))
    (let ((org-pomodoro-state :long-break)
          (org-pomodoro-end-time (time-add now 15))
          (org-pomodoro-timer :long-timer)
          (org-pomodoro-break-finished-hook
           (list (lambda () (push 'break-hook events))))
          (org-pomodoro-long-break-finished-hook
           (list (lambda () (push 'long-hook events)))))
      (org-pomodoro-long-break-finished)
      (push (list 'long-state (neomacs-org-pomodoro-test-state now)) events))
    (nreverse events)))
"###;
    let expected = expect![[
        r#"OK ((clock-out nil t) (cancelled :short-timer) (notify "Short break finished." "Ready for another pomodoro?") (sound :short-break) break-hook short-hook (short-state (:state :none :active nil :count 0 :remaining nil :timer :short-timer :mode-line nil :registered nil)) (clock-out nil t) (cancelled :long-timer) (notify "Long break finished." "Ready for another pomodoro?") (sound :long-break) break-hook long-hook (long-state (:state :none :active nil :count 0 :remaining nil :timer :long-timer :mode-line nil :registered nil)))"#
    ]];
    ParityBatchCase::value(
        "short_and_long_break_completion_clock_out_reset_and_run_hooks_in_order",
        elisp_form,
        expected,
    )
}

fn audio_delivery_prefers_native_wav_then_shell_quotes_the_configured_player() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((sound-file (expand-file-name "audio/focus bell.wav" user-emacs-directory))
       (org-pomodoro-play-sounds t)
       (org-pomodoro-audio-player "/opt/audio player")
       (org-pomodoro-finished-sound sound-file)
       (org-pomodoro-finished-sound-args "--volume 40")
       (org-pomodoro-finished-sound-p t)
       native external disabled)
  (cl-letf (((symbol-function 'sound-wav-play)
             (lambda (file) (push file native))))
    (org-pomodoro-maybe-play-sound :pomodoro))
  (let ((real-fboundp (symbol-function 'fboundp)))
    (cl-letf (((symbol-function 'fboundp)
               (lambda (symbol)
                 (if (eq symbol 'sound-wav-play)
                     nil
                   (funcall real-fboundp symbol))))
              ((symbol-function 'start-process-shell-command)
               (lambda (name buffer command)
                 (push (list name buffer command) external))))
      (org-pomodoro-maybe-play-sound :pomodoro)))
  (let ((org-pomodoro-play-sounds nil))
    (cl-letf (((symbol-function 'sound-wav-play)
               (lambda (file) (push file disabled)))
              ((symbol-function 'start-process-shell-command)
               (lambda (&rest arguments) (push arguments disabled))))
      (org-pomodoro-play-sound :pomodoro)))
  (list :selected
        (mapcar (lambda (type)
                  (list type (org-pomodoro-sound-p type)
                        (file-name-nondirectory (org-pomodoro-sound type))
                        (org-pomodoro-sound-args type)))
                '(:pomodoro :short-break :long-break))
        :native (nreverse native)
        :external (nreverse external)
        :disabled disabled
        :unknown
        (neomacs-org-pomodoro-test-error
         (lambda () (org-pomodoro-sound-p :meeting)))))
"###;
    let expected = expect![[
        r#"OK (:selected ((:pomodoro t "focus bell.wav" "--volume 40") (:short-break t "bell.wav" nil) (:long-break t "bell_multiple.wav" nil)) :native ("[ORACLE-HOME]/.emacs.d/audio/focus bell.wav") :external (("org-pomodoro-audio-player" nil "/opt/audio player --volume 40 [ORACLE-HOME]/.emacs.d/audio/focus\\ bell.wav")) :disabled nil :unknown (:error error :data ("Unknown org-pomodoro sound: :meeting") :message "Unknown org-pomodoro sound: :meeting"))"#
    ]];
    ParityBatchCase::value(
        "audio_delivery_prefers_native_wav_then_shell_quotes_the_configured_player",
        elisp_form,
        expected,
    )
}

fn countdown_formatting_and_expiry_boundary_drive_a_predictable_status_display() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((now (seconds-to-time 1800000000))
       (org-pomodoro-time-format "%.2m:%.2s")
       (org-pomodoro-format "Focus~%s")
       (org-pomodoro-overtime-format "+%s")
       (org-pomodoro-short-break-format "Pause~%s")
       (org-pomodoro-long-break-format "Recover~%s")
       (org-pomodoro-expiry-time 120)
       displays)
  (cl-letf (((symbol-function 'current-time) (lambda () now))
            ((symbol-function 'force-mode-line-update) (lambda (&rest _))))
    (dolist (phase `((:pomodoro ,(time-add now 65))
                     (:overtime ,(time-subtract now 125))
                     (:short-break ,(time-add now 9))
                     (:long-break ,(time-add now 3600))))
      (setq org-pomodoro-state (car phase)
            org-pomodoro-end-time (cadr phase))
      (org-pomodoro-update-mode-line)
      (push (list org-pomodoro-state
                  (org-pomodoro-remaining-seconds)
                  (org-pomodoro-format-seconds)
                  (neomacs-org-pomodoro-test-mode-line))
            displays))
    (setq org-pomodoro-state :none
          org-pomodoro-end-time nil)
    (org-pomodoro-update-mode-line)
    (list :displays (nreverse displays)
          :inactive (neomacs-org-pomodoro-test-mode-line)
          :expiry
          (mapcar
           (lambda (minutes)
             (setq org-pomodoro-last-clock-in
                   (time-subtract now (* minutes 60)))
             (list minutes (org-pomodoro-expires-p)))
           '(119 120 121)))))
"###;
    let expected = expect![[
        r#"OK (:displays ((:pomodoro 65.0 "01:05" (:text "[Focus~01:05] " :face org-pomodoro-mode-line)) (:overtime -125.0 "02:05" (:text "[+02:05] " :face org-pomodoro-mode-line-overtime)) (:short-break 9.0 "00:09" (:text "[Pause~00:09] " :face org-pomodoro-mode-line-break)) (:long-break 3600.0 "60:00" (:text "[Recover~60:00] " :face org-pomodoro-mode-line-break))) :inactive nil :expiry ((119 nil) (120 nil) (121 t)))"#
    ]];
    ParityBatchCase::value(
        "countdown_formatting_and_expiry_boundary_drive_a_predictable_status_display",
        elisp_form,
        expected,
    )
}

#[test]
fn org_pomodoro_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(ORG_POMODORO_MELPA_PIN, "org-pomodoro.el")
            .expect("prepare revision-pinned Org Pomodoro below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "org-pomodoro-package-batch",
        "Org Pomodoro",
        &[
            package_contract_exposes_the_task_timer_and_user_policy(),
            starting_and_resetting_a_focus_session_manage_timer_and_mode_line_lifecycle(),
            a_real_org_task_clocks_in_for_the_focus_session_and_out_for_its_break(),
            an_expired_fourth_focus_tick_enters_a_long_break_and_runs_operational_hooks(),
            manual_overtime_waits_for_the_user_before_starting_the_next_break(),
            stopping_a_running_session_honors_confirmation_and_clock_retention_policy(),
            short_and_long_break_completion_clock_out_reset_and_run_hooks_in_order(),
            audio_delivery_prefers_native_wav_then_shell_quotes_the_configured_player(),
            countdown_formatting_and_expiry_boundary_drive_a_predictable_status_display(),
        ],
    );
}
