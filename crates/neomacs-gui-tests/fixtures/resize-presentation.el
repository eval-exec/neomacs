;;; resize-presentation.el --- Resize presentation GUI fixture -*- lexical-binding: t -*-

;; A saturated mode-line makes a stale presentation observable without
;; comparing platform-dependent fonts or antialiasing pixel-for-pixel.
(set-face-attribute 'default nil :foreground "#000000" :background "#ffffff")
(set-face-attribute 'mode-line nil :foreground "#ffffff" :background "#ff0000")
(set-face-attribute 'mode-line-inactive nil :foreground "#ffffff" :background "#ff0000")
;; Native test-driver dimensions are pixels, including in the GNU control.
(setq frame-resize-pixelwise t)

(switch-to-buffer (get-buffer-create "*neomacs-resize-presentation*"))
(erase-buffer)
(unless (getenv "NEOMACS_GUI_RESIZE_AFTER_IDLE")
  (dotimes (row 100)
    (insert (format "%03d %s\n"
                    row
                    (make-string 180 (+ ?A (% row 26)))))))
(goto-char (point-min))
(setq-local truncate-lines nil)
(setq-local mode-line-format '(" RESIZE PRESENTATION "))

(when (getenv "NEOMACS_GUI_RESIZE_SPLIT")
  (split-window-below -6))

(let ((ready-path (getenv "NEOMACS_GUI_RESIZE_READY")))
  (when ready-path
    (let ((notify-ready
           (lambda ()
             (make-directory (file-name-directory ready-path) t)
             (with-temp-file ready-path
               (insert "ready\n")))))
      (if (getenv "NEOMACS_GUI_RESIZE_AFTER_IDLE")
          (progn
            (blink-cursor-mode -1)
            (run-with-idle-timer 1 nil notify-ready))
        (funcall notify-ready)))))

;; Keep the process bounded if its Rust test driver exits unexpectedly.
(run-at-time 90 nil (lambda () (kill-emacs 2)))
