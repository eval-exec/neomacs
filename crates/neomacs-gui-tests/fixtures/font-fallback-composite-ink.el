;;; font-fallback-composite-ink.el --- Fallback-composition ink regression -*- lexical-binding: t -*-
;;
;; A default face whose font has no Arabic coverage must still render an
;; automatic (shaped) Arabic composition through the font that actually
;; provides the glyphs.  The GUI test drives this with a private fontconfig
;; containing only Ubuntu Mono and Noto Sans Arabic.
;;
;; The whole scenario is timer-driven: no synthetic key events are needed, so
;; the captured frame cannot race with keyboard focus.

(run-at-time
 1 nil
 (lambda ()
   (switch-to-buffer (get-buffer-create "*font-fallback-composite-ink*"))
   (setq-local mode-line-format nil cursor-type nil)
   (set-face-attribute 'default nil
                       :font (font-spec :family "Ubuntu Mono" :size 22))
   (insert "Latin سلام Latin\n")
   (goto-char (point-min))
   (redisplay t)))

(run-at-time
 3 nil
 (lambda ()
   (redisplay t)
   (neomacs--write-frame-snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
   (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
              (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png")
              t)
   (kill-emacs 0)))
