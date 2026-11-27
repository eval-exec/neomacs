;;; distant-foreground.el --- :distant-foreground on a GUI frame (#853) -*- lexical-binding: t; -*-

;; Three rows of glyphs, each under an overlay face over a text-property face,
;; as in the #853 report (lsp-mode's highlight over a font-lock face):
;;   near     #f0f0f0 on #c6c7c7 with :distant-foreground #1b2229
;;   plain    #f0f0f0 on #c6c7c7 without :distant-foreground
;;   far      #4078f2 on #c6c7c7 with :distant-foreground #1b2229
;; GNU paints the near row's glyphs #1b2229: `load_face_colors' swaps in the
;; distant color when `color_distance' (bg, fg) is under
;; `face-near-same-color-threshold' (src/xfaces.c:1417-1424).  The plain and
;; far rows keep their own foreground.
;;
;; After a redisplay has held for 1 s the fixture exits.  GNU writes its frame
;; to NEOMACS_DEBUG_SURFACE_READBACK_PNG with `x-export-frames'; Neomacs's
;; runtime writes its surface readback to the same variable's path.
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(setq-default cursor-type nil)
;; A large font paints solid stems, so each glyph color has many exact pixels.
(set-face-attribute 'default nil :foreground "#383a42" :background "#fafafa"
                    :height 300)
(set-face-attribute 'font-lock-function-name-face nil :foreground "#4078f2")
(switch-to-buffer (get-buffer-create "*distant-foreground*"))
(setq-local mode-line-format nil)
(dolist (row '(("MMMMMMMMMMMMMMMMMMMM"
                (:foreground "#f0f0f0" :background "#c6c7c7" :distant-foreground "#1b2229"))
               ("WWWWWWWWWWWWWWWWWWWW"
                (:foreground "#f0f0f0" :background "#c6c7c7"))
               ("HHHHHHHHHHHHHHHHHHHH"
                (:foreground "#4078f2" :background "#c6c7c7" :distant-foreground "#1b2229"))))
  (let ((start (point)))
    (insert (car row))
    (put-text-property start (point) 'face 'font-lock-function-name-face)
    (overlay-put (make-overlay start (point)) 'face (cadr row))
    (insert "\n")))
(goto-char (point-max))

(defun neo-distant-finish ()
  (condition-case err
      (progn
        (redisplay t)
        (when (and (eq window-system 'x) (fboundp 'x-export-frames))
          (let ((coding-system-for-write 'no-conversion))
            (with-temp-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
              (set-buffer-multibyte nil)
              (insert (x-export-frames nil 'png)))))
        (kill-emacs 0))
    (error (princ (format "DISTANT-FAIL %S\n" err) 'external-debugging-output)
           (kill-emacs 1))))

(run-at-time 1.5 nil #'neo-distant-finish)
