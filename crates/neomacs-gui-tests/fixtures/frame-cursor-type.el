;;; frame-cursor-type.el --- frame cursor-type set after startup (#882) -*- lexical-binding: t; -*-

;; With `cursor-type' t the frame's `cursor-type' parameter decides the
;; shape.  The fixture draws a red box cursor, sets the frame parameter to
;; `bar' once the frame is up, redisplays, and exits.  GNU redraws the
;; cursor as a bar: `set_frame_cursor_types' sets `cursor_type_changed'
;; (src/xdisp.c:34597-34621).
;;
;; With NEOMACS_FCT_SHAPE set, the fixture instead sets `cursor-type' to
;; that form (read as Lisp) on a space, for the shape's exact size:
;; GNU paints an hbar one pixel shorter than its glyph and clamps a bar to
;; the glyph's width (`x_draw_bar_cursor', src/xterm.c).
;;
;; GNU writes its frame to NEOMACS_DEBUG_SURFACE_READBACK_PNG with
;; `x-export-frames', which includes the cursor; Neomacs's runtime writes its
;; surface readback to the same path.  Only the last image counts.
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(set-face-attribute 'default nil :height 300)
(set-face-background 'cursor "#ff0000")
(switch-to-buffer (get-buffer-create "*frame-cursor-type*"))
(setq-local mode-line-format nil)
(insert "MMMM    MMMM\n")
(goto-char 7)

(defun neo-fct-export ()
  (when (and (eq window-system 'x) (fboundp 'x-export-frames))
    (let ((coding-system-for-write 'no-conversion))
      (with-temp-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
        (set-buffer-multibyte nil)
        (insert (x-export-frames nil 'png))))))

(run-at-time 1.0 nil
             (lambda ()
               (let ((shape (getenv "NEOMACS_FCT_SHAPE")))
                 (if shape
                     (setq cursor-type (car (read-from-string shape)))
                   (set-frame-parameter nil 'cursor-type 'bar)))))
(run-at-time 2.0 nil
             (lambda ()
               (condition-case err
                   (progn (redisplay t) (neo-fct-export) (kill-emacs 0))
                 (error (princ (format "FCT-FAIL %S\n" err) 'external-debugging-output)
                        (kill-emacs 1)))))
