;;; header-line-recenter.el --- #873 header-line/recenter/mode-line parity -*- lexical-binding: t; -*-

;; Records GNU Emacs 31.1 and Neomacs window chrome heights, body height,
;; and `recenter' `window-start' across five state-gated phases (zero timers
;; or sleeps):
;;   1. before-redisplay: after setting `mode-line' :height 1.4 and
;;      `header-line-format', before any redisplay;
;;   2. after-recenter: after moving to line 98 col 5 and calling `(recenter)',
;;      still before redisplay;
;;   3. after-redisplay: after `(redisplay t)' measures the 1.4 mode-line and
;;      header-line;
;;   4. recenter-again: after calling `(recenter)' and `(redisplay t)' again;
;;   5. header-cleared-before-redisplay: after setting `header-line-format' to
;;      nil without an intervening redisplay.

(defun neo-hlr-emit (line)
  "Write LINE to stderr and append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-hlr-record (phase)
  "Emit one geometry record for PHASE."
  (neo-hlr-emit
   (format "HEADER-LINE-RECENTER %s %d %d %d %d %d %d"
           phase
           (frame-char-height)
           (window-mode-line-height)
           (window-header-line-height)
           (window-body-height)
           (window-body-height nil t)
           (line-number-at-pos (window-start)))))

(defun neo-hlr-run ()
  "Run the #873 five-phase observation synchronously once the frame is ready."
  (condition-case err
      (progn
        ;; Establish a settled initial presentation with a default mode line
        ;; and no header line.
        (redisplay t)
        (let ((buf (get-buffer-create "hlr-873")))
          (switch-to-buffer buf)
          (delete-other-windows)
          (fundamental-mode)
          (setq truncate-lines nil)
          (erase-buffer)
          (dotimes (i 97)
            (insert (format "line %d of the walk\n" (1+ i))))
          (insert "abcdefghij\nx\n0123456789\n")
          (dotimes (i 20)
            (insert (format "tail %d\n" (1+ i))))
          (set-face-attribute 'mode-line nil :height 1.4)
          (setq header-line-format "cursor row walk")
          (neo-hlr-record "before-redisplay")
          (goto-char (point-min))
          (forward-line 97)
          (move-to-column 5)
          (recenter)
          (neo-hlr-record "after-recenter")
          (redisplay t)
          (neo-hlr-record "after-redisplay")
          (recenter)
          (redisplay t)
          (neo-hlr-record "recenter-again")
          (setq header-line-format nil)
          (neo-hlr-record "header-cleared-before-redisplay")
          ;; At 96 DPI, default :height 104 opens 14 px (17 px line height),
          ;; while 1.4 mode-line merges in tenths of a point:
          ;; trunc(104 * 1.4) = 145 -> 19 px (23 px height) instead of
          ;; round(14 * 1.4) = 20 px (24 px height).
          (set-face-attribute 'default nil :height 104)
          (setq header-line-format "cursor row walk")
          (redisplay t)
          (neo-hlr-record "default-104-mode-line-1.4"))
        (neo-hlr-emit "COMMIT-EDITING-PASS")
        (kill-emacs 0))
    (error
     (ignore-errors (neo-hlr-emit (format "COMMIT-EDITING-FAIL %S" err)))
     (kill-emacs 1))))

(add-hook 'window-setup-hook #'neo-hlr-run)
