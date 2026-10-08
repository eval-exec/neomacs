;;; Stalled TLS must keep the timer and display loop alive. -*- lexical-binding: t; -*-
(require 'timer)
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(defun stalled-tls-snapshot (suffix)
  (redisplay t)
  (let ((path (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")))
    (when (and path (fboundp 'neomacs--write-frame-snapshot))
      (neomacs--write-frame-snapshot (concat path suffix) nil 'json))))
(run-with-timer
 0.5 nil
 (lambda ()
   (condition-case error-data
       (let* ((buffer (get-buffer-create "*stalled TLS*"))
              (server (make-network-process :name "tls-blackhole" :server t :host "127.0.0.1" :service t
                                            :noquery t :filter (lambda (_process _bytes))))
              (client (make-network-process :name "tls-client" :host "127.0.0.1"
                                            :service (process-contact server :service) :noquery t))
              (timer-fired nil)
              (timer (run-with-timer
                      0.025 nil
                      (lambda ()
                        (setq timer-fired t)
                        (with-current-buffer buffer (insert "TIMER-DURING-TLS\n"))
                        (stalled-tls-snapshot ".during")))))
         (switch-to-buffer buffer)
         (erase-buffer)
         (insert "TLS-NEGOTIATING\n")
         (unwind-protect
             (let* ((outcome (catch 'tls-timeout
                               (with-timeout (0.15 (throw 'tls-timeout 'timeout))
                                 (gnutls-boot client 'gnutls-x509pki
                                              '(:hostname "localhost" :complete-negotiation t)))
                               'returned))
                    (closed (not (process-live-p client)))
                    (passed (and timer-fired (eq outcome 'timeout) closed)))
               (insert (if passed "TLS-TIMEOUT-CLOSED\n" "TLS-FAILED\n"))
               (stalled-tls-snapshot "")
               (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
                 (insert (format "{\"passed\":%s,\"timer\":%s,\"outcome\":\"%s\",\"closed\":%s}\n"
                                 (if passed "true" "false") (if timer-fired "true" "false")
                                 outcome (if closed "true" "false"))))
               ;; Allow the native presentation and terminal parser to observe
               ;; the finished frame before the test exits.
               (run-with-timer 0.5 nil (lambda () (kill-emacs (if passed 0 1)))))
           (cancel-timer timer)
           (dolist (process (process-list))
             (when (string-prefix-p "tls-" (process-name process)) (delete-process process)))))
     (error (message "Stalled TLS fixture: %S" error-data) (kill-emacs 1)))))
;; This is a harness watchdog, not a TLS implementation timeout.
(run-with-timer 8 nil (lambda () (kill-emacs 2)))
