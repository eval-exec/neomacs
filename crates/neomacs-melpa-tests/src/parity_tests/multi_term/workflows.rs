use expect_test::expect;

use super::ParityBatchCase;

fn terminal_round_trip_uses_real_term_process_and_keymap() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-term-test-with-workspace
  (multi-term)
  (let* ((buffer (current-buffer))
         (process (get-buffer-process buffer)))
    (neomacs-multi-term-test-wait-for-output buffer "READY\n")
    (term-send-raw-string "deploy release-42")
    (call-interactively (key-binding (kbd "C-m")))
    (neomacs-multi-term-test-wait-for-output buffer "ECHO:deploy release-42\n")
    (list :buffer (buffer-name buffer)
          :managed (mapcar #'buffer-name multi-term-buffer-list)
          :mode major-mode
          :mode-name mode-name
          :char-mode (and (term-in-char-mode) t)
          :return-binding (lookup-key (current-local-map) (kbd "C-m"))
          :process (list :live (and (process-live-p process) t)
                         :status (process-status process)
                         :command (file-name-nondirectory
                                   (car (process-command process))))
          :text (neomacs-multi-term-test-buffer-text buffer))))
"####;
    let expected = expect![[
        r#"OK (:buffer "*terminal<1>*" :managed ("*terminal<1>*") :mode term-mode :mode-name "Term" :char-mode t :return-binding term-send-return :process (:live t :status run :command "sh") :text "READY\nECHO:deploy release-42\n")"#
    ]];
    ParityBatchCase::value(
        "terminal_round_trip_uses_real_term_process_and_keymap",
        elisp_form,
        expected,
    )
}

fn next_and_previous_cycle_live_terminal_sessions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-term-test-with-workspace
  (dotimes (_ 3)
    (multi-term)
    (neomacs-multi-term-test-wait-for-output (current-buffer) "READY\n"))
  (let ((sessions
         (mapcar
          (lambda (buffer)
            (with-current-buffer buffer
              (list :name (buffer-name)
                    :mode major-mode
                    :live (and (process-live-p (get-buffer-process buffer)) t)
                    :text (neomacs-multi-term-test-buffer-text buffer))))
          multi-term-buffer-list))
        transitions)
    (switch-to-buffer (car multi-term-buffer-list))
    (push (cons 'start (buffer-name)) transitions)
    (multi-term-next)
    (push (cons 'next (buffer-name)) transitions)
    (multi-term-next 2)
    (push (cons 'next-two (buffer-name)) transitions)
    (multi-term-prev)
    (push (cons 'previous (buffer-name)) transitions)
    (list :sessions sessions
          :transitions (nreverse transitions))))
"####;
    let expected = expect![[
        r#"OK (:sessions ((:name "*terminal<1>*" :mode term-mode :live t :text "READY\n") (:name "*terminal<2>*" :mode term-mode :live t :text "READY\n") (:name "*terminal<3>*" :mode term-mode :live t :text "READY\n")) :transitions ((start . "*terminal<1>*") (next . "*terminal<2>*") (next-two . "*terminal<1>*") (previous . "*terminal<3>*")))"#
    ]];
    ParityBatchCase::value(
        "next_and_previous_cycle_live_terminal_sessions",
        elisp_form,
        expected,
    )
}

fn dedicated_terminal_hides_and_reopens_the_same_live_session() -> ParityBatchCase {
    let elisp_form = r####"
(condition-case unexpected-condition
    (neomacs-multi-term-test-with-workspace
  (let ((origin (selected-window)))
    (multi-term-dedicated-open)
    (let* ((buffer multi-term-dedicated-buffer)
           (first-window multi-term-dedicated-window)
           (process (get-buffer-process buffer)))
      (neomacs-multi-term-test-wait-for-output buffer "READY\n")
      (with-current-buffer buffer
        (term-send-raw-string "status production\r"))
      (neomacs-multi-term-test-wait-for-output buffer "ECHO:status production\n")
      (let ((opened
             (list :selected-origin (eq (selected-window) origin)
                   :exists (and (multi-term-dedicated-exist-p) t)
                   :buffer (buffer-name buffer)
                   :window-dedicated (and (window-dedicated-p first-window) t)
                   :window-buffer (buffer-name (window-buffer first-window))
                   :process-live (and (process-live-p process) t)
                   :mode (buffer-local-value 'major-mode buffer)
                   :text (neomacs-multi-term-test-buffer-text buffer))))
        (multi-term-dedicated-select)
        (let ((selected
               (list :selected-window (eq (selected-window) first-window)
                     :window-buffer (buffer-name
                                     (window-buffer (selected-window)))
                     :same-buffer (eq (window-buffer (selected-window)) buffer)
                     :process-live (and (process-live-p process) t))))
          (select-window origin)
          (multi-term-dedicated-toggle)
          (let ((hidden
                 (list :exists (and (multi-term-dedicated-exist-p) t)
                       :buffer-live (and (buffer-live-p buffer) t)
                       :process-live (and (process-live-p process) t)
                       :old-window-live (and (window-live-p first-window) t)
                       :selected-origin (eq (selected-window) origin))))
            (multi-term-dedicated-toggle)
            (let ((reopened
                   (list :exists (and (multi-term-dedicated-exist-p) t)
                         :same-buffer (eq multi-term-dedicated-buffer buffer)
                         :new-window (not (eq multi-term-dedicated-window first-window))
                         :window-dedicated
                         (and (window-dedicated-p multi-term-dedicated-window) t)
                         :process-live (and (process-live-p process) t)
                         :selected-origin (eq (selected-window) origin)
                         :text (neomacs-multi-term-test-buffer-text buffer))))
              (multi-term-dedicated-close)
              (list :opened opened
                    :selected selected
                    :hidden hidden
                    :reopened reopened
                    :closed (list :exists (and (multi-term-dedicated-exist-p) t)
                                  :buffer-live (and (buffer-live-p buffer) t)
                                  :process-live (and (process-live-p process) t)
                                  :selected-origin (eq (selected-window) origin))))))))))
  (error (list :unexpected-condition unexpected-condition)))
"####;
    let expected = expect![[
        r#"OK (:opened (:selected-origin t :exists t :buffer "*MULTI-TERM-DEDICATED*" :window-dedicated t :window-buffer "*MULTI-TERM-DEDICATED*" :process-live t :mode term-mode :text "READY\nECHO:status production\n") :selected (:selected-window t :window-buffer "*MULTI-TERM-DEDICATED*" :same-buffer t :process-live t) :hidden (:exists nil :buffer-live t :process-live t :old-window-live nil :selected-origin t) :reopened (:exists t :same-buffer t :new-window t :window-dedicated t :process-live t :selected-origin t :text "READY\nECHO:status production\n") :closed (:exists nil :buffer-live t :process-live t :selected-origin t))"#
    ]];
    ParityBatchCase::value(
        "dedicated_terminal_hides_and_reopens_the_same_live_session",
        elisp_form,
        expected,
    )
}

fn process_exit_kills_the_session_and_selects_the_survivor() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-term-test-with-workspace
  (multi-term)
  (let ((victim (current-buffer)))
    (neomacs-multi-term-test-wait-for-output victim "READY\n")
    (multi-term)
    (let ((survivor (current-buffer)))
      (neomacs-multi-term-test-wait-for-output survivor "READY\n")
      (switch-to-buffer victim)
      (term-send-raw-string "quit\r")
      (neomacs-multi-term-test-wait-until
       (lambda () (not (buffer-live-p victim)))
       "Multi-term to remove the exited session")
      (list :victim-live (and (buffer-live-p victim) t)
            :survivor (list :selected
                            (eq (window-buffer (selected-window)) survivor)
                            :name (buffer-name survivor)
                            :mode (buffer-local-value 'major-mode survivor)
                            :process-live
                            (and (process-live-p (get-buffer-process survivor)) t)
                            :text (neomacs-multi-term-test-buffer-text survivor))
            :managed (mapcar #'buffer-name multi-term-buffer-list)))))
"####;
    let expected = expect![[
        r#"OK (:victim-live nil :survivor (:selected t :name "*terminal<2>*" :mode term-mode :process-live t :text "READY\n") :managed ("*terminal<2>*"))"#
    ]];
    ParityBatchCase::value(
        "process_exit_kills_the_session_and_selects_the_survivor",
        elisp_form,
        expected,
    )
}

fn killing_a_terminal_buffer_terminates_its_live_process() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-term-test-with-workspace
  (multi-term)
  (let* ((victim (current-buffer))
         (process (get-buffer-process victim)))
    (neomacs-multi-term-test-wait-for-output victim "READY\n")
    (multi-term)
    (let ((survivor (current-buffer)))
      (neomacs-multi-term-test-wait-for-output survivor "READY\n")
      (switch-to-buffer victim)
      (set-process-query-on-exit-flag process nil)
      (kill-buffer victim)
      (neomacs-multi-term-test-wait-until
       (lambda () (not (process-live-p process)))
       "killing the terminal buffer to terminate its child process")
      (list :victim-live (and (buffer-live-p victim) t)
            :process (list :live (and (process-live-p process) t)
                           :status (process-status process)
                           :exit-status (process-exit-status process))
            :survivor (list :selected
                            (eq (window-buffer (selected-window)) survivor)
                            :name (buffer-name survivor)
                            :mode (buffer-local-value 'major-mode survivor)
                            :process-live
                            (and (process-live-p (get-buffer-process survivor)) t))
            :managed (mapcar #'buffer-name multi-term-buffer-list)))))
"####;
    let expected = expect![[
        r#"OK (:victim-live nil :process (:live nil :status signal :exit-status 1) :survivor (:selected t :name "*terminal<2>*" :mode term-mode :process-live t) :managed ("*terminal<2>*"))"#
    ]];
    ParityBatchCase::value(
        "killing_a_terminal_buffer_terminates_its_live_process",
        elisp_form,
        expected,
    )
}

fn missing_terminal_program_terminates_and_removes_the_session() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-multi-term-test-with-workspace
  (setq multi-term-program (expand-file-name "missing-terminal-program" root))
  (multi-term)
  (let* ((buffer (current-buffer))
         (process (get-buffer-process buffer)))
    (neomacs-multi-term-test-wait-until
     (lambda () (not (buffer-live-p buffer)))
     "Multi-term to remove the session whose program cannot start")
    (list :buffer-live (and (buffer-live-p buffer) t)
          :managed (mapcar #'buffer-name multi-term-buffer-list)
          :process-status (process-status process)
          :exit-status (process-exit-status process))))
"####;
    let expected =
        expect!["OK (:buffer-live nil :managed nil :process-status exit :exit-status 127)"];
    ParityBatchCase::value(
        "missing_terminal_program_terminates_and_removes_the_session",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        terminal_round_trip_uses_real_term_process_and_keymap(),
        next_and_previous_cycle_live_terminal_sessions(),
        dedicated_terminal_hides_and_reopens_the_same_live_session(),
        process_exit_kills_the_session_and_selects_the_survivor(),
        killing_a_terminal_buffer_terminates_its_live_process(),
        missing_terminal_program_terminates_and_removes_the_session(),
    ]
}
