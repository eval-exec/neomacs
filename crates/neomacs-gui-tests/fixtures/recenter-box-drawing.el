;;; recenter-box-drawing.el --- Recenter with noncanonical row heights -*- lexical-binding: t -*-

(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil scroll-margin 0)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)
(set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 210)
(set-frame-size nil 680 700 t)

(defvar neomacs-recenter-stage 0)
(defvar neomacs-recenter-point nil)
(defvar neomacs-recenter-trace nil)
(defvar neomacs-recenter-readback-deadline 0)

(defun neomacs-recenter-fail (error)
  (princ (format "Recenter GUI fixture failed: %S\n" error)
         'external-debugging-output)
  (kill-emacs 1))

(defun neomacs-recenter-await-readback ()
  ;; The harness asserts on the renderer's surface-readback PNG, which the
  ;; render thread writes on a later present.  Wait for the artifact itself
  ;; instead of racing it with a fixed delay.
  (condition-case error
      (let* ((png (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG"))
             (written (and png
                           (file-regular-p png)
                           (> (file-attribute-size (file-attributes png)) 0))))
        (cond
         ((or (null png) written) (kill-emacs 0))
         ((> (float-time) neomacs-recenter-readback-deadline)
          (neomacs-recenter-fail
           (format "surface readback PNG was not written: %S" png)))
         (t (run-at-time 0.02 nil #'neomacs-recenter-await-readback))))
    (error (neomacs-recenter-fail error))))

(defun neomacs-recenter-finish ()
  (condition-case error
      (progn
        (set-buffer (window-buffer (selected-window)))
        (unless (= (point) neomacs-recenter-point)
          (error "Recenter moved point"))
        (redisplay t)
        (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
          (insert
           (json-encode
            `((original_point . ,neomacs-recenter-point)
              (point . ,(window-point))
              (trace . ,(vconcat (reverse neomacs-recenter-trace)))))))
        (neomacs--write-frame-snapshot
         (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
        (neomacs--write-frame-snapshot
         (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") t 'text)
        (setq neomacs-recenter-readback-deadline (+ (float-time) 10))
        (run-at-time 0.02 nil #'neomacs-recenter-await-readback))
    (error (neomacs-recenter-fail error))))

(defun neomacs-recenter-after-command ()
  (condition-case error
      (progn
        (set-buffer (window-buffer (selected-window)))
        (unless (= (point) neomacs-recenter-point)
          (error "Recenter moved point"))
        (push (symbol-name recenter-last-op) neomacs-recenter-trace)
        (if (= neomacs-recenter-stage 3)
            ;; Observe another settled frame, so automatic visibility recovery
            ;; cannot make a transient bottom placement appear successful.
            (run-at-time 0.2 nil #'neomacs-recenter-finish)
          (neomacs-recenter-command)))
    (error (neomacs-recenter-fail error))))

(defun neomacs-recenter-command ()
  (condition-case error
      (progn
        (set-buffer (window-buffer (selected-window)))
        ;; Resolve the public C-l binding and supply successive command history.
        ;; Separate execute-kbd-macro calls reset that history; timers avoid
        ;; inhibiting redisplay as a single long keyboard macro would.
        (let* ((command (key-binding (kbd "C-l")))
               (this-command command)
               (last-command (and (> neomacs-recenter-stage 0) command)))
          (setq neomacs-recenter-stage (1+ neomacs-recenter-stage))
          (call-interactively command))
        (run-at-time 0.2 nil #'neomacs-recenter-after-command))
    (error (neomacs-recenter-fail error))))

(defun neomacs-recenter-start ()
  (condition-case error
      (progn
        (switch-to-buffer (get-buffer-create "*recenter-box-drawing*"))
        (delete-other-windows)
        (fundamental-mode)
        (setq-local truncate-lines t)
        ;; A controlled row-height difference makes the pixel/line distinction
        ;; deterministic without depending on a platform's fallback box font.
        (setq-local line-spacing 3)
        (dotimes (index 120)
          (insert (format "%03d: │├└ box-drawing recenter row\n" (1+ index))))
        (goto-char (point-min))
        (forward-line 59)
        (setq neomacs-recenter-point (point))
        (run-at-time 0.3 nil #'neomacs-recenter-command))
    (error (neomacs-recenter-fail error))))

(run-at-time 0.5 nil #'neomacs-recenter-start)
