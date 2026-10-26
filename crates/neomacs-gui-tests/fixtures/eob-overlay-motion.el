;;; eob-overlay-motion.el --- Exercise the initialized display iterator -*- lexical-binding: t -*-
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil)
(defun eob-overlay-motion-probe ()
  (let ((observations nil)
        (origin (or (getenv "NEOMACS_MOTION_PROBE_ORIGIN") "eob")))
    (switch-to-buffer (get-buffer-create "*eob-overlay-motion*"))
    (dolist (property '(before-string after-string))
      ;; Seventy rows exceed the initial motion query and the visible window.
      ;; Reaching the source anchor alone must not freeze that partial query.
      (dolist (tail (list "\nX" "\nX\nY" "\nX\n"
                          (apply #'concat (make-list 70 "\nX"))))
        (dolist (narrowed '(nil t))
          (widen)
          (erase-buffer)
          (remove-overlays)
          (if narrowed
              (progn (insert "PRE\nabc\ndef\nPOST") (narrow-to-region 5 12))
            (insert "abc\ndef"))
          (setq-local mode-line-format nil header-line-format nil
                      tab-line-format nil truncate-lines nil word-wrap nil)
          (let ((overlay (make-overlay (point-max) (point-max))))
            (overlay-put overlay property tail))
          (goto-char (point-max))
          (when (equal origin "bol") (beginning-of-line))
          (redisplay t)
          (let ((result
                 (condition-case err
                     (list (vertical-motion 1) (point) (point-min) (point-max))
                   (error (list 'error (error-message-string err))))))
            (push (format "%S" (list property tail narrowed result)) observations)))))
    (redisplay t)
    (when (and (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
               (fboundp 'neomacs--write-frame-snapshot))
      (neomacs--write-frame-snapshot
       (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode `((origin . ,origin)
                             (observations . ,(vconcat (nreverse observations)))))))
    (run-at-time 0.3 nil (lambda () (kill-emacs 0)))))
(run-at-time 0.5 nil #'eob-overlay-motion-probe)
