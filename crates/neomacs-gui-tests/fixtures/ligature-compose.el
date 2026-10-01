;;; ligature-compose.el --- Issue #447 GUI verification -*- lexical-binding: t -*-
;; Renders "->" with a composition-function-table rule (font-shape-gstring)
;; and writes the frame snapshot + surface readback for pixel comparison.
(run-at-time
 1 nil
 (lambda ()
   (switch-to-buffer (get-buffer-create "*ligature*"))
   (setq-local mode-line-format nil cursor-type nil)
   (set-face-attribute 'default nil :font
                       (font-spec :family "DejaVu Sans Mono" :size 30))
   (setq auto-composition-mode t
         auto-composition-function 'auto-compose-chars
         composition-function-table (make-char-table nil))
    (aset composition-function-table ?-
          '(["\\(?:->\\)" 0 font-shape-gstring]))
   (let ((enable (getenv "NEOMACS_LIGATURE_ENABLED")))
     (if (equal enable "0")
         (setq composition-function-table (make-char-table nil))))
   (insert (propertize "a -> b" 'face '(:foreground "black" :background "white")))
   (goto-char (point-min))
   ;; Global binding: the verification target is the rendered surface, not
   ;; which buffer owns the local map at keypress time. GNU's
   ;; composition_gstring_adjust_zero_width analog lands in the shaping core.
   (global-set-key
    (kbd "C-c t")
    (lambda ()
      (interactive)
      (redisplay t)
      (neomacs--write-frame-snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
      (run-at-time 1 nil
       (lambda ()
         (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                    (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
         (kill-emacs 0)))))
   (with-temp-file (getenv "NEOMACS_SELECTION_READY")
     (insert "ready\n")
     (prin1 (list :auto-comp-mode auto-composition-mode
                  :auto-comp-fn auto-composition-function
                  :table-rule (aref composition-function-table ?-)
                  :buffer (buffer-name))
            (current-buffer)))
   ;; Force a redisplay AFTER the rule is armed so the display walk consults
   ;; the composition table with the rule present, then dump the same state
   ;; again post-redisplay for the diag file.
   (redisplay t)
   (run-at-time 2 nil
     (lambda ()
       (let ((diag (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".diag")))
         (with-temp-file diag
           (prin1 (list :auto-comp-mode auto-composition-mode
                        :table-rule (aref composition-function-table ?-)))))
       (kill-emacs 0)))))
