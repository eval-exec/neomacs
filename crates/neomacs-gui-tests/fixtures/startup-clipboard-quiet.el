;;; startup-clipboard-quiet.el --- a plain GUI start with an empty clipboard (#889) -*- lexical-binding: t; -*-

;; Nothing owns the selection on the test display.  The fixture reads the
;; clipboard once more, the way `yank' does, then exits; the row checks the
;; startup log.  An empty or unseen selection is an absence, not a failed
;; read: the read answers nil, and no WARN may appear.
(run-at-time 2.0 nil
             (lambda ()
               (condition-case err
                   (progn
                     (princ (format "STARTUP-CLIPBOARD %S\n" (gui-get-selection 'CLIPBOARD))
                            'external-debugging-output)
                     (kill-emacs 0))
                 (error
                  (princ (format "STARTUP-CLIPBOARD-ERROR %S\n" err) 'external-debugging-output)
                  (kill-emacs 1)))))
