use expect_test::expect;

use super::ParityBatchCase;

fn controller_runs_pause_resume_and_repeatable_abort_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(let ((post-command-hook '(neomacs-auto-pause-test--observer))
      scheduled cancelled abort pause resume after-pause after-resume result
      (messages-start
       (with-current-buffer (messages-buffer) (point-max))))
  (setq neomacs-auto-pause-test--events nil)
  (cl-letf (((symbol-function 'run-with-idle-timer)
             (lambda (delay repeat function &rest arguments)
               (setq scheduled (list delay repeat function arguments))
               :idle-timer-handle))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer cancelled))))
    (setq abort
          (auto-pause
           (lambda () (push :pause neomacs-auto-pause-test--events))
           (lambda () (push :resume neomacs-auto-pause-test--events))
           2.5))
    (setq pause (nth 2 scheduled))
    (funcall pause)
    (funcall pause)
    (setq resume
          (car
           (cl-remove-if-not
            (lambda (entry)
              (eq (neomacs-auto-pause-test--symbol-role entry) 'resume))
            post-command-hook)))
    (setq after-pause
          (list
           :events (nreverse (copy-sequence neomacs-auto-pause-test--events))
           :hook (mapcar #'neomacs-auto-pause-test--symbol-role post-command-hook)
           :resume-count
           (neomacs-auto-pause-test--hook-count resume post-command-hook)))
    (run-hooks 'post-command-hook)
    (setq after-resume
          (list
           :events (nreverse (copy-sequence neomacs-auto-pause-test--events))
           :hook (mapcar #'neomacs-auto-pause-test--symbol-role post-command-hook)))
    (funcall abort)
    (funcall abort)
    (setq result
          (list
           :scheduled
           (list (nth 0 scheduled)
                 (nth 1 scheduled)
                 (neomacs-auto-pause-test--symbol-role (nth 2 scheduled))
                 (nth 3 scheduled))
           :controller (functionp abort)
           :after-pause after-pause
           :after-resume after-resume
           :after-abort
           (list
            :cancelled (nreverse cancelled)
            :pause-bound (fboundp pause)
            :resume-bound (fboundp resume)
            :hook (mapcar #'neomacs-auto-pause-test--symbol-role post-command-hook))
           :messages (neomacs-auto-pause-test--messages messages-start))))
  result)
"####;
    let expect = expect![[
        r#"OK (:scheduled (2.5 t pause nil) :controller t :after-pause (:events (:pause :pause) :hook (resume neomacs-auto-pause-test--observer) :resume-count 1) :after-resume (:events (:pause :pause :resume :observer) :hook (neomacs-auto-pause-test--observer)) :after-abort (:cancelled (:idle-timer-handle :idle-timer-handle) :pause-bound nil :resume-bound nil :hook (neomacs-auto-pause-test--observer)) :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "abort auto-pause-abort-<id> [2 times]"))"#
    ]];
    ParityBatchCase::value(
        "controller_runs_pause_resume_and_repeatable_abort_lifecycle",
        elisp_form,
        expect,
    )
}

fn macro_leaks_global_advice_and_marks_a_later_unrelated_process() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((inside (neomacs-auto-pause-test--pipe "auto-pause-inside"))
       (outside (neomacs-auto-pause-test--pipe "auto-pause-outside"))
       (after-cleanup (neomacs-auto-pause-test--pipe "auto-pause-after-cleanup"))
       (queue (list inside outside after-cleanup))
       timers cancelled result
       (messages-start
        (with-current-buffer (messages-buffer) (point-max))))
  (neomacs-auto-pause-test--remove-package-advice)
  (unwind-protect
      (cl-letf (((symbol-function 'start-process)
                 (lambda (&rest _arguments) (pop queue)))
                ((symbol-function 'run-with-idle-timer)
                 (lambda (delay repeat function &rest arguments)
                   (let ((timer
                          (list :timer (1+ (length timers))
                                delay repeat function arguments)))
                     (push timer timers)
                     timer)))
                ((symbol-function 'cancel-timer)
                 (lambda (timer) (push timer cancelled))))
        (let ((body-value
               (with-auto-pause 11
                 (start-process "inside" nil "ignored"))))
          (let ((retained
                 (list
                  (neomacs-auto-pause-test--advice-names 'start-process)
                  (neomacs-auto-pause-test--advice-names
                   'set-process-sentinel))))
            (start-process "outside" nil "ignored")
            (neomacs-auto-pause-test--remove-package-advice)
            (start-process "after-cleanup" nil "ignored")
            (setq result
                  (list
                   :body-value (eq body-value inside)
                   :retained-advice retained
                   :marked
                   (mapcar
                    (lambda (process)
                      (and (auto-pause-process-p process) t))
                    (list inside outside after-cleanup))
                   :timers
                   (mapcar
                    (lambda (timer)
                      (list
                       (nth 1 timer)
                       (nth 2 timer)
                       (nth 3 timer)
                       (neomacs-auto-pause-test--symbol-role (nth 4 timer))
                       (nth 5 timer)))
                    (nreverse timers))
                   :advice-after-cleanup
                   (list
                    (neomacs-auto-pause-test--advice-names 'start-process)
                    (neomacs-auto-pause-test--advice-names
                     'set-process-sentinel))
                   :messages
                   (neomacs-auto-pause-test--messages messages-start))))))
    (neomacs-auto-pause-test--remove-package-advice)
    (dolist (process (list inside outside after-cleanup))
      (when-let ((controller (process-get process 'auto-pause-abort-function)))
        (ignore-errors (funcall controller)))
      (neomacs-auto-pause-test--delete-process process)))
  result)
"####;
    let expect = expect![[
        r#"OK (:body-value t :retained-advice ((("auto-pause-advise-start-process")) (("auto-pause-advise-set-process-sentinel"))) :marked (t t nil) :timers ((1 11 t pause nil) (2 11 t pause nil)) :advice-after-cleanup (nil nil) :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>"))"#
    ]];
    ParityBatchCase::value(
        "macro_leaks_global_advice_and_marks_a_later_unrelated_process",
        elisp_form,
        expect,
    )
}

fn real_worker_stops_resumes_and_finishes_through_public_macro() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-pause-real-worker"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (program (neomacs-auto-pause-test--write-worker root))
       (ready (expand-file-name "ready" root))
       (release (expand-file-name "release" root))
       (buffer (get-buffer-create " *auto-pause-real-worker*"))
       (process-connection-type nil)
       (post-command-hook nil)
       (timers-before (copy-sequence timer-idle-list))
       process timer pause stopped resumed after-resume result
       (messages-start
        (with-current-buffer (messages-buffer) (point-max))))
  (setq neomacs-auto-pause-test--events nil)
  (neomacs-auto-pause-test--remove-package-advice)
  (unwind-protect
      (progn
        (with-auto-pause 60
          (setq process
                (start-process "auto-pause-real-worker" buffer
                               program ready release))
          (set-process-sentinel process #'neomacs-auto-pause-test--sentinel))
        (setq result
              (list
               :retained-advice
               (list
                (neomacs-auto-pause-test--advice-names 'start-process)
                (neomacs-auto-pause-test--advice-names
                 'set-process-sentinel))))
        (neomacs-auto-pause-test--remove-package-advice)
        (unless (neomacs-auto-pause-test--wait-file ready process)
          (error "worker did not become ready"))
        (setq timer
              (car (cl-set-difference timer-idle-list timers-before :test #'eq)))
        (setq pause (timer--function timer))
        (funcall pause)
        (setq stopped (neomacs-auto-pause-test--wait-status process 'stop))
        (setq result
              (append
               result
               (list
                :after-pause
                (list
                 :stopped stopped
                 :status (process-status process)
                 :hook
                 (mapcar #'neomacs-auto-pause-test--symbol-role
                         post-command-hook)))))
        (run-hooks 'post-command-hook)
        (setq resumed (neomacs-auto-pause-test--wait-status process 'run))
        (setq after-resume
              (list
               :resumed resumed
               :status (process-status process)
               :hook
               (mapcar #'neomacs-auto-pause-test--symbol-role
                       post-command-hook)))
        (with-temp-file release)
        (neomacs-auto-pause-test--wait-status process 'exit)
        (accept-process-output nil 0.05)
        (setq result
              (append
               result
               (list
                :after-resume
                after-resume
                :finished
                (list
                 :status (process-status process)
                 :exit-code (process-exit-status process)
                 :events (nreverse neomacs-auto-pause-test--events)
                 :output (neomacs-auto-pause-test--buffer-text buffer)
                 :marked (and (auto-pause-process-p process) t)
                 :timer-live (and (memq timer timer-idle-list) t)
                 :pause-bound (fboundp pause))
                :messages
                (neomacs-auto-pause-test--messages messages-start)))))
    (neomacs-auto-pause-test--remove-package-advice)
    (when (and process (process-live-p process))
      (ignore-errors (delete-process process)))
    (when (and process
               (functionp
                (process-get process 'auto-pause-abort-function)))
      (ignore-errors
        (funcall (process-get process 'auto-pause-abort-function))))
    (when (timerp timer)
      (ignore-errors (cancel-timer timer)))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (when (file-exists-p root)
      (delete-directory root t)))
  result)
"####;
    let expect = expect![[
        r#"OK (:retained-advice ((("auto-pause-advise-start-process")) (("auto-pause-advise-set-process-sentinel"))) :after-pause (:stopped t :status stop :hook (resume)) :after-resume (:resumed t :status run :hook nil) :finished (:status exit :exit-code 7 :events ((:user "stopped (signal)\n") (:user "run") (:user "exited abnormally with code 7\n")) :output "payload Ω\n" :marked t :timer-live nil :pause-bound nil) :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "abort auto-pause-abort-<id>"))"#
    ]];
    ParityBatchCase::value(
        "real_worker_stops_resumes_and_finishes_through_public_macro",
        elisp_form,
        expect,
    )
}

fn body_error_is_preserved_while_the_malformed_advices_remain_installed() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((child (neomacs-auto-pause-test--pipe "auto-pause-error-child"))
       timers cancelled outcome result
       (messages-start
        (with-current-buffer (messages-buffer) (point-max))))
  (neomacs-auto-pause-test--remove-package-advice)
  (unwind-protect
      (cl-letf (((symbol-function 'start-process)
                 (lambda (&rest _arguments) child))
                ((symbol-function 'run-with-idle-timer)
                 (lambda (delay repeat function &rest arguments)
                   (let ((timer (list :timer delay repeat function arguments)))
                     (push timer timers)
                     timer)))
                ((symbol-function 'cancel-timer)
                 (lambda (timer) (push timer cancelled))))
        (setq outcome
              (condition-case error-data
                  (with-auto-pause 4
                    (start-process "child" nil "ignored")
                    (error "body exploded Ω"))
                (error (list (car error-data) (cdr error-data)))))
        (setq result
              (list
               :outcome outcome
               :marked (and (auto-pause-process-p child) t)
               :timer
               (let ((timer (car timers)))
                 (list (nth 1 timer)
                       (nth 2 timer)
                       (neomacs-auto-pause-test--symbol-role (nth 3 timer))
                       (nth 4 timer)))
               :retained-advice
               (list
                (neomacs-auto-pause-test--advice-names 'start-process)
                (neomacs-auto-pause-test--advice-names
                 'set-process-sentinel))
               :messages (neomacs-auto-pause-test--messages messages-start))))
    (neomacs-auto-pause-test--remove-package-advice)
    (when-let ((controller (process-get child 'auto-pause-abort-function)))
      (ignore-errors (funcall controller)))
    (neomacs-auto-pause-test--delete-process child))
  result)
"####;
    let expect = expect![[
        r#"OK (:outcome (error ("body exploded Ω")) :marked t :timer (4 t pause nil) :retained-advice ((("auto-pause-advise-start-process")) (("auto-pause-advise-set-process-sentinel"))) :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>"))"#
    ]];
    ParityBatchCase::value(
        "body_error_is_preserved_while_the_malformed_advices_remain_installed",
        elisp_form,
        expect,
    )
}

fn sentinel_aborts_only_on_exit_and_still_cleans_up_after_user_error() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (file-name-as-directory
         (expand-file-name "auto-pause-real-sentinels"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (signaled-root (expand-file-name "signaled" sandbox))
       (failing-root (expand-file-name "failing" sandbox))
       (signaled-program
        (neomacs-auto-pause-test--write-worker signaled-root))
       (failing-program
        (neomacs-auto-pause-test--write-worker failing-root))
       (signaled-ready (expand-file-name "ready" signaled-root))
       (signaled-release (expand-file-name "release" signaled-root))
       (failing-ready (expand-file-name "ready" failing-root))
       (failing-release (expand-file-name "release" failing-root))
       (process-connection-type nil)
       (timers-before (copy-sequence timer-idle-list))
       signaled failing signaled-timer failing-timer signaled-pause failing-pause
       signaled-controller failing-wrapper failing-outcome after-signal
       after-signal-cleanup before-failing-callback after-failing-callback result
       (messages-start
        (with-current-buffer (messages-buffer) (point-max))))
  (setq neomacs-auto-pause-test--events nil)
  (unwind-protect
      (progn
        (setq signaled
              (start-process "auto-pause-signaled" nil signaled-program
                             signaled-ready signaled-release))
        (set-process-query-on-exit-flag signaled nil)
        (set-process-sentinel signaled #'neomacs-auto-pause-test--sentinel)
        (auto-pause-mark-process signaled 60)
        (setq signaled-controller
              (process-get signaled 'auto-pause-abort-function))
        (unless (neomacs-auto-pause-test--wait-file signaled-ready signaled)
          (error "signaled worker did not become ready"))
        (setq signaled-timer
              (car (cl-set-difference timer-idle-list timers-before :test #'eq)))
        (setq signaled-pause (timer--function signaled-timer))
        (signal-process signaled 'SIGKILL)
        (neomacs-auto-pause-test--wait-status signaled 'signal)
        (setq after-signal
              (list
               :status (process-status signaled)
               :exit-code (process-exit-status signaled)
               :events (nreverse (copy-tree neomacs-auto-pause-test--events))
               :timer-live (and (memq signaled-timer timer-idle-list) t)
               :pause-bound (fboundp signaled-pause)
               :marked (and (auto-pause-process-p signaled) t)))
        (funcall signaled-controller)
        (setq after-signal-cleanup
              (list
               :timer-live (and (memq signaled-timer timer-idle-list) t)
               :pause-bound (fboundp signaled-pause)
               :marked (and (auto-pause-process-p signaled) t)))

        (setq timers-before (copy-sequence timer-idle-list))
        (setq failing
              (start-process "auto-pause-failing" nil failing-program
                             failing-ready failing-release))
        (set-process-query-on-exit-flag failing nil)
        (set-process-sentinel failing
                              #'neomacs-auto-pause-test--failing-sentinel)
        (auto-pause-mark-process failing 60)
        (setq failing-wrapper (process-sentinel failing))
        (set-process-sentinel failing nil)
        (unless (neomacs-auto-pause-test--wait-file failing-ready failing)
          (error "failing-sentinel worker did not become ready"))
        (setq failing-timer
              (car (cl-set-difference timer-idle-list timers-before :test #'eq)))
        (setq failing-pause (timer--function failing-timer))
        (with-temp-file failing-release)
        (neomacs-auto-pause-test--wait-status failing 'exit)
        (accept-process-output nil 0.05)
        (setq before-failing-callback
              (list
               :status (process-status failing)
               :exit-code (process-exit-status failing)
               :timer-live (and (memq failing-timer timer-idle-list) t)
               :pause-bound (fboundp failing-pause)
               :marked (and (auto-pause-process-p failing) t)))
        (setq failing-outcome
              (condition-case error-data
                  (funcall failing-wrapper failing
                           "exited abnormally with code 7\n")
                (error (list (car error-data) (cdr error-data)))))
        (setq after-failing-callback
              (list
               :outcome failing-outcome
               :events (nreverse (copy-tree neomacs-auto-pause-test--events))
               :timer-live (and (memq failing-timer timer-idle-list) t)
               :pause-bound (fboundp failing-pause)
               :marked (and (auto-pause-process-p failing) t)))
        (setq result
              (list
               :after-signal after-signal
               :after-signal-cleanup after-signal-cleanup
               :before-failing-callback before-failing-callback
               :after-failing-callback after-failing-callback
               :messages (neomacs-auto-pause-test--messages messages-start))))
    (when (and signaled
               (functionp
                (process-get signaled 'auto-pause-abort-function)))
      (ignore-errors
        (funcall (process-get signaled 'auto-pause-abort-function))))
    (when (and failing
               (functionp
                (process-get failing 'auto-pause-abort-function)))
      (ignore-errors
        (funcall (process-get failing 'auto-pause-abort-function))))
    (when (timerp signaled-timer)
      (ignore-errors (cancel-timer signaled-timer)))
    (when (timerp failing-timer)
      (ignore-errors (cancel-timer failing-timer)))
    (neomacs-auto-pause-test--delete-process signaled)
    (neomacs-auto-pause-test--delete-process failing)
    (when (file-exists-p sandbox)
      (delete-directory sandbox t)))
  result)
"####;
    let expect = expect![[
        r#"OK (:after-signal (:status signal :exit-code 9 :events ((:user "killed\n")) :timer-live t :pause-bound t :marked t) :after-signal-cleanup (:timer-live nil :pause-bound nil :marked t) :before-failing-callback (:status exit :exit-code 7 :timer-live t :pause-bound t :marked t) :after-failing-callback (:outcome (error ("user sentinel failed: exited abnormally with code 7\n")) :events ((:user "killed\n") (:failing-user "exited abnormally with code 7\n")) :timer-live nil :pause-bound nil :marked t) :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "abort auto-pause-abort-<id>" "expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "abort auto-pause-abort-<id>"))"#
    ]];
    ParityBatchCase::value(
        "sentinel_aborts_only_on_exit_and_still_cleans_up_after_user_error",
        elisp_form,
        expect,
    )
}

fn abort_while_paused_leaves_an_unbound_resume_hook() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((process (neomacs-auto-pause-test--pipe "auto-pause-abort-paused"))
       (post-command-hook '(neomacs-auto-pause-test--observer))
       scheduled cancelled signals controller pause resume hook-outcome result
       (messages-start
        (with-current-buffer (messages-buffer) (point-max))))
  (setq neomacs-auto-pause-test--events nil)
  (unwind-protect
      (cl-letf (((symbol-function 'run-with-idle-timer)
                 (lambda (delay repeat function &rest arguments)
                   (setq scheduled (list delay repeat function arguments))
                   :paused-timer))
                ((symbol-function 'cancel-timer)
                 (lambda (timer) (push timer cancelled)))
                ((symbol-function 'signal-process)
                 (lambda (_process signal &optional _current-group)
                   (push signal signals)
                   0)))
        (auto-pause-mark-process process 8)
        (setq controller (process-get process 'auto-pause-abort-function))
        (setq pause (nth 2 scheduled))
        (funcall pause)
        (setq resume
              (car
               (cl-remove-if-not
                (lambda (entry)
                  (eq (neomacs-auto-pause-test--symbol-role entry) 'resume))
                post-command-hook)))
        (funcall controller)
        (setq hook-outcome
              (condition-case error-data
                  (run-hooks 'post-command-hook)
                (void-function
                 (list
                  (car error-data)
                  (neomacs-auto-pause-test--symbol-role
                   (cadr error-data))))))
        (setq result
              (list
               :scheduled
               (list (nth 0 scheduled)
                     (nth 1 scheduled)
                     (neomacs-auto-pause-test--symbol-role (nth 2 scheduled))
                     (nth 3 scheduled))
               :signals (nreverse signals)
               :cancelled (nreverse cancelled)
               :resume-bound (fboundp resume)
               :hook-outcome hook-outcome
               :hook-after-error
               (mapcar #'neomacs-auto-pause-test--symbol-role post-command-hook)
               :observer-ran (and (memq :observer neomacs-auto-pause-test--events) t)
               :still-marked (and (auto-pause-process-p process) t)
               :messages (neomacs-auto-pause-test--messages messages-start))))
    (setq post-command-hook nil)
    (neomacs-auto-pause-test--delete-process process))
  result)
"####;
    let expect = expect![[
        r#"OK (:scheduled (8 t pause nil) :signals (SIGSTOP) :cancelled (:paused-timer) :resume-bound nil :hook-outcome (void-function resume) :hook-after-error (resume neomacs-auto-pause-test--observer) :observer-ran nil :still-marked t :messages ("expand auto-pause-pause-<id> auto-pause-resume-<id> auto-pause-abort-<id> auto-pause-idle-timer-<id>" "abort auto-pause-abort-<id>"))"#
    ]];
    ParityBatchCase::value(
        "abort_while_paused_leaves_an_unbound_resume_hook",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        controller_runs_pause_resume_and_repeatable_abort_lifecycle(),
        macro_leaks_global_advice_and_marks_a_later_unrelated_process(),
        real_worker_stops_resumes_and_finishes_through_public_macro(),
        body_error_is_preserved_while_the_malformed_advices_remain_installed(),
        sentinel_aborts_only_on_exit_and_still_cleans_up_after_user_error(),
        abort_while_paused_leaves_an_unbound_resume_hook(),
    ]
}
