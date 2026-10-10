;;; XPM named colors under a real frame. -*- lexical-binding: t; -*-

;; Runs identically under GNU Emacs and Neomacs on a window-system frame. GNU
;; resolves an XPM `c` value through the frame terminal's `defined_color_hook`
;; (src/image.c:6503-6511), and the same hook backs `color-values' -- so what
;; the editor says a name means and what an XPM carrying that name must render
;; as are one claim. This fixture paints one swatch per name and records the
;; editor's own `color-values' for it; the Rust side checks the rendered pixels
;; against both (issue #545).

(require 'json)
(require 'subr-x)

(defconst xpm-named-color-cases
  '("gray14" "gray50" "gray75" "green" "maroon" "light blue"))

(defconst xpm-named-color-swatch-size 48)

(defun xpm-named-color-swatch (value)
  "One XPM image filled with VALUE, taken as the XPM color value itself."
  (let ((row (concat "\"" (make-string xpm-named-color-swatch-size ?a) "\""))
        (side (number-to-string xpm-named-color-swatch-size)))
    (create-image
     (concat "/* XPM */\nstatic char *swatch[] = {\n\""
             side " " side " 1 1\",\n"
             "\"a c " value "\",\n"
             (mapconcat #'identity
                        (make-list xpm-named-color-swatch-size row) ",\n")
             "\n};\n")
     'xpm t :ascent 'center)))

(defun xpm-named-colors-probe ()
  (let ((buffer (get-buffer-create "*xpm-named-colors*")))
    (switch-to-buffer buffer)
    (erase-buffer)
    (setq-local cursor-type nil)
    (setq-local mode-line-format nil)
    (setq-local header-line-format nil)
    (dolist (case xpm-named-color-cases)
      (insert (format "%-11s" case))
      (insert-image (xpm-named-color-swatch case))
      (insert "\n"))
    (goto-char (point-min))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert
       (json-encode
        (append
         (list (cons "native-engine"
                     (if (fboundp 'neomacs--write-frame-snapshot) t :json-false))
               (cons "graphic" (if (display-graphic-p) t :json-false)))
         ;; GNU answers 16-bit channels; the Rust side reduces them like the
         ;; renderer does.
         (mapcar (lambda (case) (cons case (color-values case)))
                 xpm-named-color-cases)))))
    (redisplay t)
    ;; Let the compositor present the painted buffer before the process is
    ;; asked to exit; `kill-emacs' during startup hangs under GTK.
    (run-at-time
     2 nil
     (lambda ()
       (when (fboundp 'neomacs--write-frame-snapshot)
         (neomacs--write-frame-snapshot
          (concat (getenv "NEOMACS_GUI_STATE_JSON") ".frame.json") nil 'json))
       (kill-emacs 0)))))

(condition-case err
    (xpm-named-colors-probe)
  (error
   (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
     (insert (prin1-to-string err)))
   (kill-emacs 1)))
