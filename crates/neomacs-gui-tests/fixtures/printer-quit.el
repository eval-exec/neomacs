;;; printer-quit.el --- Interactive printer interruption -*- lexical-binding: t -*-

(switch-to-buffer (get-buffer-create "*printer-quit*"))
(lisp-interaction-mode)
(erase-buffer)

(run-at-time
 0.3 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (display-graphic-p) (error "Expected a graphical frame"))
         ;; The unreadable marker callback requests quit while the printer is
         ;; traversing the vector. Timer handlers bind inhibit-quit, so enable
         ;; delivery explicitly as an interactive command would. The following
         ;; list must observe quit before prin1-to-string returns.
         (insert "(let ((print-circle nil) (inhibit-quit nil)"
                 " (print-unreadable-function"
                 " (lambda (_object _escape) (setq quit-flag t) \"Q\")))"
                 " (condition-case nil"
                 " (prin1-to-string (vector (make-marker) '(after)))"
                 " (quit (list 'caught 'inside 'printer))))")
         (call-interactively #'eval-print-last-sexp)
         (unless (string-match-p "(caught inside printer)" (buffer-string))
           (error "Printer did not deliver quit to its surrounding handler: %S"
                  (buffer-string)))
         (insert "(list (not quit-flag) (+ 40 2))")
         (call-interactively #'eval-print-last-sexp)
         (unless (string-match-p "(t 42)" (buffer-string))
           (error "Evaluation did not recover after printer quit"))
         (redisplay t)
         (neomacs--write-frame-snapshot
          (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
         (neomacs--write-frame-snapshot
          (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") t 'text)
         (run-at-time 0.3 nil (lambda () (kill-emacs 0))))
     (quit (princ "Printer-quit GUI regression: quit escaped printing\n"
                  'external-debugging-output)
           (kill-emacs 1))
     (error (princ (format "Printer-quit GUI regression: %S\n" err)
                   'external-debugging-output)
            (kill-emacs 1)))))
