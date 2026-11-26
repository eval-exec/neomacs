;;; bidi-gui-zero-width-785.el --- GUI zero-width ink (issue #785) -*- lexical-binding: t; -*-
;;
;; One line whose painted red-band geometry is measured.  R2_TEXT selects the
;; text; each "control" variant puts one zero-width character into an
;; otherwise identical "ab" line, so the wrapper can compare the painted text
;; extent's delta against GNU's.  GNU displays format controls through the
;; default `glyphless-char-display-control' (format-control -> thin-space: a
;; 1-pixel advance), and a zero-advance character outside that table keeps a
;; zero-width glyph while the GUI pen advances one pixel
;; (src/xdisp.c:33755-33760, "all visible glyphs have at least 1-pixel
;; width", applied after append_glyph).  A full character cell for any of
;; these characters is a Neo-only gap.
;;
;; R2_TEXT  plain | rlm | lrm | zwnj | zwj | leading-combining | combining-after

(menu-bar-mode -1)
(tool-bar-mode -1)
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))
(setq inhibit-startup-screen t
      initial-scratch-message nil)

(let ((buf (get-buffer-create "*bidi-zero-width*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert
     (pcase (getenv "R2_TEXT")
       ("rlm" (decode-coding-string
               (unibyte-string #x61 #xe2 #x80 #x8f #x62 #x0a) 'utf-8))
       ("lrm" (decode-coding-string
               (unibyte-string #x61 #xe2 #x80 #x8e #x62 #x0a) 'utf-8))
       ("zwnj" (decode-coding-string
                (unibyte-string #x61 #xe2 #x80 #x8c #x62 #x0a) 'utf-8))
       ("zwj" (decode-coding-string
               (unibyte-string #x61 #xe2 #x80 #x8d #x62 #x0a) 'utf-8))
       ;; U+0301 COMBINING ACUTE ACCENT before the base letters: GNU gives it
       ;; a zero-width glyph and advances the pen one pixel.
       ("leading-combining" (decode-coding-string
                             (unibyte-string #xcc #x81 #x61 #x62 #x0a) 'utf-8))
       ("combining-after" (decode-coding-string
                           (unibyte-string #x61 #xcc #x81 #x62 #x0a) 'utf-8))
       (_ "ab\n")))
    ;; The red band is the painted text extent the wrapper measures.
    (add-face-text-property (point-min) (1- (point-max)) '(:background "red"))
    ;; Point at EOL: the cursor then sits on the unfaced newline and cannot
    ;; paint ink or background inside the measured red band.
    (goto-char (point-max))
    (set-buffer-modified-p nil))
  (switch-to-buffer buf))

(when (eq (window-system) 'x)
  (set-frame-size (selected-frame) 60 10))
(redisplay t)

;; The wrapper screenshots and kills the process; this bounds a stuck run.
(run-at-time 30 nil #'kill-emacs)
