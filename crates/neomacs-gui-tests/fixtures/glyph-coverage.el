;;; glyph-coverage.el --- Coverage polarity regression -*- lexical-binding: t -*-
(run-at-time
 1 nil
 (lambda ()
   (switch-to-buffer (get-buffer-create "*glyph-coverage*"))
   (setq-local mode-line-format nil cursor-type nil)
   (set-face-attribute 'default nil :font
                       (font-spec :family (or (getenv "NEOMACS_COVERAGE_FONT") "DejaVu Sans Mono") :size 30))
   (let ((text "Hamburgefontsiv 0123456789"))
     (insert (propertize text 'face '(:foreground "black" :background "white")) "\n"
             (propertize text 'face '(:foreground "white" :background "black")) "\n"
             (propertize text 'face '(:foreground "#204060" :background "#dfbf9f")) "\n"
             (propertize text 'face '(:foreground "#dfbf9f" :background "#204060")) "\n"))
   (goto-char (point-min))
   (local-set-key
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
   (with-temp-file (getenv "NEOMACS_SELECTION_READY") (insert "ready"))))
