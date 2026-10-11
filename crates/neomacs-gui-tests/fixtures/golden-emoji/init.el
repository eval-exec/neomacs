;;; Golden-sheet probe: COLRv1 emoji, one per line, one size. -*- lexical-binding: t -*-
;;; The readback diagnostics only report colorful faces, so every cell carries
;;; a non-grayscale foreground; the golden records whatever this fixture
;;; defines, so it only has to stay stable, not pretty.
(require 'json)

(defun golden-emoji-sheet ()
  (switch-to-buffer (get-buffer-create "*golden-emoji*"))
  (set-frame-font (font-spec :family "Spleen" :size 16))
  (erase-buffer)
  ;; Four glyphs: the readback diagnostics report at most four boxes.
  (dolist (ch '("\U0001F347" ;; grape: layered purples
                "\U0001F34E" ;; apple: red over greens
                "\U0001F308" ;; rainbow: gradient arcs
                "\U0001F33B")) ;; sunflower: yellow over green
    (insert (propertize ch 'face '(:foreground "blue")))
    (insert "\n"))
  (redisplay t))

;; Run in the hook itself, not on a timer: the surface readback dumps a fixed
;; number of frames from startup, and a timer would race that window.
(add-hook 'window-setup-hook #'golden-emoji-sheet)
(add-hook 'window-setup-hook (lambda () (run-at-time 8 nil #'kill-emacs 0)) 90)
;;; init.el ends here
