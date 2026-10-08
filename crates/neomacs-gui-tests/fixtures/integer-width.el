;;; integer-width.el --- Interactive arithmetic recovery -*- lexical-binding: t -*-

(switch-to-buffer (get-buffer-create "*integer-width*"))
(lisp-interaction-mode)
(erase-buffer)

(run-at-time
 0.3 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (display-graphic-p) (error "Expected a graphical frame"))
         (insert "(condition-case e (let ((integer-width 128)) (expt 2 128)) (error e))")
         (call-interactively #'eval-print-last-sexp)
         (unless (string-match-p "(overflow-error)" (buffer-string))
           (error "Interactive evaluation did not signal overflow"))
         (insert "(list integer-width (+ 40 2))")
         (call-interactively #'eval-print-last-sexp)
         (unless (string-match-p "(65536 42)" (buffer-string))
           (error "Interactive evaluation did not restore the width binding"))
         (redisplay t)
         (neomacs--write-frame-snapshot
          (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
         (neomacs--write-frame-snapshot
          (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") t 'text)
         (run-at-time 0.3 nil (lambda () (kill-emacs 0))))
     (error (message "Integer-width GUI regression: %S" err) (kill-emacs 1)))))
