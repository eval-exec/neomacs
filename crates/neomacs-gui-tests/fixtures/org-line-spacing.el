;;; org-line-spacing.el --- Stable row geometry across point/region changes -*- lexical-binding: t -*-

(setq default-text-properties '(line-spacing 0.14)
      native-comp-jit-compilation nil
      native-comp-deferred-compilation nil)
(require 'org)
(custom-set-faces '(org-block ((t (:background "gray93")))))
(defvar neomacs-spacing-phase 0)
(defvar neomacs-spacing-timer nil)

(defun neomacs-spacing-capture (stage)
  (redisplay t)
  (neomacs--write-frame-snapshot
   (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") "." stage) nil 'json)
  (run-at-time
   0.5 nil
   (lambda ()
     (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") "." stage ".png") t))))

(defun neomacs-spacing-step ()
  (condition-case err
      (progn
        (when (> neomacs-spacing-phase 0) (set-buffer "*org-line-spacing*"))
        (pcase neomacs-spacing-phase
          (0
           (switch-to-buffer (get-buffer-create "*org-line-spacing*"))
           (set-face-attribute 'default nil :font
                               (font-spec :family "DejaVu Sans Mono" :size 20))
           (org-mode)
           (insert "* Spacing reproduction\nPlain alpha\nPlain beta\n#+BEGIN_SRC R\nlibrary(DiagrammeR)\n\nedges <- data[-1, ]\n\nfaction_colors <- c(red = 'pink')\n#+END_SRC\nTrailing text\n")
           (font-lock-ensure)
           (goto-char (point-min))
           (set-window-point nil (point))
           (setq mark-active nil))
          (1 (neomacs-spacing-capture "before"))
          (2
           (goto-char (point-min))
           (call-interactively #'next-line)
           (call-interactively #'set-mark-command)
           (dotimes (_ 8) (call-interactively #'next-line))
           (setq deactivate-mark nil)
           (set-window-point nil (point))
           (unless (and (region-active-p) (= (mark) 24) (= (point) 137))
             (error "Selection fixture did not activate the expected region")))
          (3 (neomacs-spacing-capture "selected"))
          (4
           (setq mark-active nil)
           (goto-char (point-min))
           (forward-line 5)
           (set-window-point nil (point))
           (unless (= (point) 81) (error "Point did not reach the empty line")))
          (5 (neomacs-spacing-capture "empty-cursor"))
          (6
           (neomacs--write-frame-snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
           (cancel-timer neomacs-spacing-timer)
           (kill-emacs 0)))
        (setq neomacs-spacing-phase (1+ neomacs-spacing-phase)))
    (error (message "Org line-spacing regression: %S" err) (kill-emacs 1))))

(setq neomacs-spacing-timer (run-at-time 1 1 #'neomacs-spacing-step))
