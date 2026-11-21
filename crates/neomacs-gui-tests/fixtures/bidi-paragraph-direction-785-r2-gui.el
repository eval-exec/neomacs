;;; bidi-paragraph-direction-785-r2-gui.el --- Issue #785 R2 GUI row -*- lexical-binding: t; -*-
;;
;; The reporter's file for issue #785 R2, exactly the bytes
;; d8 ac d8 a7 0a d8 ac 0a (`JA`, newline, `JIM`, newline), displayed with a
;; chosen `bidi-display-reordering` / `bidi-paragraph-direction` pair.  The
;; wrapper test screenshots the whole frame and reads where the first narrow
;; ink band (the first Arabic line) was painted, so both engines must agree on
;; the paragraph alignment.
;;
;; R2_DIR      nil | left-to-right | right-to-left
;; R2_REORDER  t | nil
;;
;; Chrome is turned off so the first narrow band is body text in both engines.

(menu-bar-mode -1)
(tool-bar-mode -1)
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))
(setq inhibit-startup-screen t
      initial-scratch-message nil)

(let ((buf (get-buffer-create "*bidi-r2*")))
  (with-current-buffer buf
    (setq bidi-display-reordering (not (equal (getenv "R2_REORDER") "nil")))
    (setq bidi-paragraph-direction
          (let ((dir (getenv "R2_DIR")))
            (unless (equal dir "nil")
              (intern dir))))
    (erase-buffer)
    (insert (decode-coding-string
             (unibyte-string #xd8 #xac #xd8 #xa7 #x0a #xd8 #xac #x0a)
             'utf-8))
    (goto-char (point-min))
    (set-buffer-modified-p nil))
  (switch-to-buffer buf))

(when (eq (window-system) 'x)
  (set-frame-size (selected-frame) 60 10))
(redisplay t)

;; The wrapper takes its screenshot and kills the process; this only bounds a
;; run that never gets that far.
(run-at-time 30 nil #'kill-emacs)
