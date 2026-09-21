;;; minibuffer-line-gui.el --- GUI repro for issue #414 (minibuffer-line) -*- lexical-binding: t; -*-
;; Load the GNU ELPA package source from this fixture's own directory
;; (`require' cannot see it: the fixtures directory is not on `load-path').
(load (expand-file-name
        "minibuffer-line.el"
        (file-name-directory (or load-file-name default-directory))))

(run-at-time
 1 nil
 (lambda ()
   (let ((snap (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT")))
     (condition-case err
         (progn
           (unless (display-graphic-p) (error "Expected a graphical frame"))
           ;; Deterministic format: the package default embeds the hostname
           ;; and wall clock, which cannot match across machines or runs.
           (setq minibuffer-line-format
                 '("" (:eval (concat "MBL" "-GUI-MARKER"))))
           (minibuffer-line-mode 1)
           ;; The mode's own update ran once; clear the lingering echo-area
           ;; load message (it occupies the miniwindow and would mask the
           ;; minibuffer-line buffer), then force a full redisplay so the
           ;; snapshot captures what the miniwindow actually renders.
           (message nil)
           (redisplay t)
           (when (and snap (fboundp 'neomacs--write-frame-snapshot))
             (make-directory (file-name-directory snap) t)
             (neomacs--write-frame-snapshot snap t 'text-faces))
           (princ "\nMBL-GUI-PASS\n" 'external-debugging-output))
       (error (progn
                (princ (format "\nMBL-GUI-FAIL: %S\n" err)
                       'external-debugging-output)
                (when (and snap (fboundp 'neomacs--write-frame-snapshot))
                  (make-directory (file-name-directory snap) t)
                  (neomacs--write-frame-snapshot snap t 'text-faces)))))
     (run-at-time 1 nil (lambda () (kill-emacs 0))))))
