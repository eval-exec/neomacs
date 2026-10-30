;;; process-timer-fairness-test.el --- Process waits yield to timers -*- lexical-binding: t -*-

;; Run with either GNU Emacs or Neomacs:
;;   --batch -Q -l test/neomacs/process-timer-fairness-test.el \
;;   -f ert-run-tests-batch-and-exit

(require 'ert)

(ert-deftest process-timer-fairness-between-ready-filters ()
  "A ready set must not postpone a due timer until all filters finish."
  (skip-unless (file-executable-p "/bin/sh"))
  (let ((process-adaptive-read-buffering nil)
        (processes nil) (timer nil) (calls 0) (calls-at-timer nil))
    (unwind-protect
        (progn
          (dotimes (i 6)
            (push (make-process
                   :name (format "timer-fairness-%d" i)
                   :connection-type 'pipe :noquery t :sentinel #'ignore
                   :command '("/bin/sh" "-c" "printf x; sleep 3")
                   :filter
                   (lambda (_process _output)
                     (setq calls (1+ calls))
                     (unless timer
                       (setq timer
                             (run-at-time 0 nil
                                          (lambda ()
                                            (setq calls-at-timer calls)))))
                     ;; Use computation: sleep-for would itself service timers.
                     (let ((end (+ (float-time) 0.025)))
                       (while (< (float-time) end)))))
                  processes))
          ;; Let all pipes become readable before servicing the ready set.
          (call-process "/bin/sleep" nil nil nil "0.15")
          (let ((deadline (+ (float-time) 2)))
            (while (and (< calls 6) (< (float-time) deadline))
              (sleep-for 0.05)))
          (should (= calls 6))
          (should (equal calls-at-timer 1)))
      (when timer (cancel-timer timer))
      (dolist (process processes)
        (when (process-live-p process) (delete-process process))))))

(ert-deftest process-timer-fairness-recursive-filter ()
  "A recursive wait must service a peer without repeating the outer filter."
  (skip-unless (file-executable-p "/bin/sh"))
  (let ((process-adaptive-read-buffering nil)
        (processes nil) (calls 0) (nested-calls nil))
    (unwind-protect
        (progn
          (dotimes (i 3)
            (push (make-process
                   :name (format "recursive-fairness-%d" i)
                   :connection-type 'pipe :noquery t :sentinel #'ignore
                   :command '("/bin/sh" "-c" "printf x; sleep 3")
                   :filter
                   (lambda (_process _output)
                     (setq calls (1+ calls))
                     (when (= calls 1)
                       (accept-process-output nil 0.5)
                       (setq nested-calls calls))))
                  processes))
          (call-process "/bin/sleep" nil nil nil "0.15")
          (let ((deadline (+ (float-time) 2)))
            (while (and (< calls 3) (< (float-time) deadline))
              (sleep-for 0.05)))
          (should (= calls 3))
          (should (> nested-calls 1)))
      (dolist (process processes)
        (when (process-live-p process) (delete-process process))))))

(ert-deftest process-timer-fairness-busy-source-yields-to-peer ()
  "Returning after one read must not starve another continuously ready fd."
  (skip-unless (file-executable-p "/bin/sh"))
  (let ((process-adaptive-read-buffering nil) (read-process-output-max 4096)
        (processes nil) (bytes 0) (bytes-at-peer nil))
    (unwind-protect
        (progn
          (push (make-process
                 :name "busy-fairness" :connection-type 'pipe
                 :coding 'binary :noquery t :sentinel #'ignore
                 :command '("/bin/sh" "-c" "head -c 1048576 /dev/zero; sleep 3")
                 :filter (lambda (_process output)
                           (setq bytes (+ bytes (length output)))))
                processes)
          (push (make-process
                 :name "peer-fairness" :connection-type 'pipe
                 :noquery t :sentinel #'ignore
                 :command '("/bin/sh" "-c" "printf x; sleep 3")
                 :filter (lambda (_process _output) (setq bytes-at-peer bytes)))
                processes)
          (call-process "/bin/sleep" nil nil nil "0.15")
          (let ((deadline (+ (float-time) 2)))
            (while (and (< bytes 1048576) (< (float-time) deadline))
              (sleep-for 0.05)))
          (should (numberp bytes-at-peer))
          (should (< bytes-at-peer 1048576))
          (should (= bytes 1048576)))
      (dolist (process processes)
        (when (process-live-p process) (delete-process process))))))

(ert-deftest process-timer-fairness-targeted-wait-yields-to-peer ()
  "Waiting for a busy target services peers unless JUST-THIS-ONE is set."
  (skip-unless (file-executable-p "/bin/sh"))
  (dolist (just-this-one '(nil t))
    (let ((process-adaptive-read-buffering nil) (read-process-output-max 4096)
          (hot nil) (peer nil) (bytes 0) (bytes-at-peer nil))
      (unwind-protect
          (progn
            (setq hot
                  (make-process
                   :name "targeted-fairness" :connection-type 'pipe
                   :coding 'binary :noquery t :sentinel #'ignore
                   :command '("/bin/sh" "-c" "head -c 1048576 /dev/zero; read line")
                   :filter
                   (lambda (_process output)
                     (setq bytes (+ bytes (length output)))
                     ;; Keep the producer ready without recursively servicing
                     ;; process output or timers from this filter.
                     (let ((end (+ (float-time) 0.001)))
                       (while (< (float-time) end))))))
            (setq peer
                  (make-process
                   :name "targeted-peer" :connection-type 'pipe
                   :noquery t :sentinel #'ignore
                   :command '("/bin/sh" "-c" "printf x; read line")
                   :filter (lambda (_process _output)
                             (setq bytes-at-peer bytes))))
            (call-process "/bin/sleep" nil nil nil "0.15")
            (let ((deadline (+ (float-time) 3)))
              (while (and (< bytes 1048576) (< (float-time) deadline))
                (accept-process-output hot 0.1 nil just-this-one)))
            (should (= bytes 1048576))
            (if just-this-one
                (progn
                  (should-not bytes-at-peer)
                  (accept-process-output peer 1 nil t)
                  (should (= bytes-at-peer 1048576)))
              (should (numberp bytes-at-peer))
              (should (< bytes-at-peer 1048576))))
        (dolist (process (list hot peer))
          (when (and process (process-live-p process))
            (delete-process process)))))))

;;; process-timer-fairness-test.el ends here
