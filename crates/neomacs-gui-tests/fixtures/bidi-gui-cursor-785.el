;;; bidi-gui-cursor-785.el --- GUI cursor on R2L rows (issue #785) -*- lexical-binding: t; -*-
;;
;; The reporter's R2L shapes, but with the point placed on a SPACE character
;; and a red background on the whole line: GNU draws a box cursor with the
;; frame's cursor colour as its background and the character face's background
;; as its ink, so a cursor over a space is a solid dark cell inside a red band.
;; The wrapper screenshots the frame and finds that cell, then compares the
;; cursor's x with GNU's.  Both engines therefore have to agree on where point
;; lands on a right-aligned row.
;;
;; R2_DIR      nil | left-to-right | right-to-left
;; R2_REORDER  t | nil
;; R2_TEXT     arabic ("JA space BA space JA") | ascii ("ab cd ef")
;; R2_POINT    1-based point; the test only uses positions on a space.
;;
;; Chrome is turned off so the red text band is the first painted row.

(menu-bar-mode -1)
(tool-bar-mode -1)
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))
(setq inhibit-startup-screen t
      initial-scratch-message nil)

;; One cursor phase while the wrapper takes its screenshots.  `cursor-type'
;; is a per-buffer variable: set the DEFAULT so the buffer opened below gets
;; a filled box (setting only the startup buffer's local value leaves the
;; displayed buffer with no cursor at all).
(blink-cursor-mode -1)
(setq-default cursor-type 'box)
(setq cursor-in-non-selected-windows nil)

(let ((buf (get-buffer-create "*bidi-cursor*")))
  (with-current-buffer buf
    (setq cursor-type 'box)
    (setq bidi-display-reordering (not (equal (getenv "R2_REORDER") "nil")))
    (setq bidi-paragraph-direction
          (let ((dir (getenv "R2_DIR")))
            (unless (equal dir "nil")
              (intern dir))))
    (erase-buffer)
    (insert
     (if (equal (getenv "R2_TEXT") "ascii")
         "ab cd ef\n"
       ;; JA, ALEF, SPACE, BEH, ALEF, SPACE, JA, newline.
       (decode-coding-string
        (unibyte-string #xd8 #xac #xd8 #xa7 #x20 #xd8 #xa8 #xd8 #xa7 #x20
                        #xd8 #xac #x0a)
        'utf-8)))
    ;; The red band marks the text area; the box cursor inverts this face, so
    ;; its cell is dark while every other text cell is red-backed.
    (add-face-text-property (point-min) (1- (point-max)) '(:background "red"))
    (goto-char (string-to-number (getenv "R2_POINT")))
    (set-buffer-modified-p nil))
  (switch-to-buffer buf))

(when (eq (window-system) 'x)
  (set-frame-size (selected-frame) 60 10))
(redisplay t)

;; The wrapper takes its screenshot and kills the process; this only bounds a
;; run that never gets that far.
(run-at-time 30 nil #'kill-emacs)
