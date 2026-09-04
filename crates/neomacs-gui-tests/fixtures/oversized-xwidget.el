;;; oversized-xwidget.el --- Oversized xwidget GUI fixture -*- lexical-binding: t -*-

(require 'xwidget)

(switch-to-buffer (get-buffer-create "*neomacs-gui-oversized-xwidget*"))
(erase-buffer)
(insert " ")
(goto-char (point-min))

;; GNU keeps the browser at this intrinsic size, crops only the glyph's row
;; advance, and clips the native view to the window text area.  Make both
;; dimensions unambiguously larger than the live text area so a missing crop
;; makes the replacement disappear under RejectOverflowingGlyph (issue #301).
(let* ((text-width (max 1 (window-body-width nil t)))
       (text-height (max 1 (window-body-height nil t)))
       (content-width (* 2 text-width))
       (content-height (* 2 text-height))
       (xwidget (xwidget-insert (point-min) 'webkit
                                "Neomacs oversized xwidget"
                                content-width content-height)))
  (setq-local neomacs-gui-oversized-xwidget xwidget)
  ;; A data URI avoids network, DNS, filesystem, and server timing.  Saturated
  ;; magenta is deliberately unlike the default Neomacs frame, so the PNG
  ;; readback can prove that WebKit content reached the compositor.
  (xwidget-webkit-goto-uri
   xwidget
   "data:text/html,%3Chtml%3E%3Cbody%20style%3D%22margin%3A0%3Bbackground%3Argb%28255%2C0%2C255%29%3Bwidth%3A100vw%3Bheight%3A100vh%22%3E%3C%2Fbody%3E%3C%2Fhtml%3E"))

(defun neomacs-gui-oversized-xwidget-capture ()
  (force-window-update)
  (redisplay t)
  (let ((snap-json (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON"))
        (snap-txt (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT")))
    (when (and snap-json (fboundp 'neomacs--write-frame-snapshot))
      (make-directory (file-name-directory snap-json) t)
      (neomacs--write-frame-snapshot snap-json t 'json))
    (when (and snap-txt (fboundp 'neomacs--write-frame-snapshot))
      (make-directory (file-name-directory snap-txt) t)
      (neomacs--write-frame-snapshot snap-txt t 'text-faces))))

;; Give WPE's process and texture handoff time to publish the deterministic
;; page before taking the semantic snapshot and the harness's final readback.
(run-at-time 1 nil #'neomacs-gui-oversized-xwidget-capture)
(run-at-time 3 nil (lambda () (kill-emacs 0)))
