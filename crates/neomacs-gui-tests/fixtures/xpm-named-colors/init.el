;;; XPM named colors under a real frame. -*- lexical-binding: t; -*-

;; Runs identically under GNU Emacs and Neomacs on a window-system frame. GNU
;; resolves an XPM `c` value through the frame terminal's `defined_color_hook`
;; (src/image.c:6503-6511), and the same hook backs `color-values' -- so what
;; the editor says a name means and what an XPM carrying that name must render
;; as are one claim. This fixture paints one swatch per name and records the
;; editor's own `color-values' for it; the Rust side checks the rendered pixels
;; against both (issue #545).
;;
;; Its second half is issue #550: a key that resolves to *nothing* is painted by
;; a rule, and the rule reads the frame's foreground -- never the face the image
;; is displayed under, never the specification's `:foreground' (GNU
;; `src/image.c:6518' and `:6537-6538'). GNU keeps the frame's `foreground-color'
;; equal to the default face's foreground (`src/xfaces.c:4394-4404'), so the
;; frame foreground is set to a colour nothing else in the frame uses.

(require 'json)
(require 'subr-x)

(defconst xpm-named-color-cases
  '("gray14" "gray50" "gray75" "green" "maroon" "light blue"))

(defconst xpm-named-color-swatch-size 48)

(defconst xpm-named-color-fallback-value "opaque"
  "A name no X11 database defines; GNU paints its pixels with the frame foreground.")

(defconst xpm-named-colors-frame-foreground "#123456"
  "Distinctive on purpose: the Rust side counts it, and nothing else paints it.")

(defconst xpm-named-colors-remap-foreground "#0a0b0c"
  "The foreground this fixture remaps the `default' face to (issue #556).
GNU's FRAME_FOREGROUND_PIXEL does not follow `face-remapping-alist', so the
remapped fallback swatch must not paint this.")

(defconst xpm-named-colors-label-face '(:foreground "black")
  "Labels stay black so the frame foreground has only the swatches to come from.")

(defun xpm-named-color-xpm (value)
  "One XPM document filled with VALUE, taken as the XPM color value itself."
  (let ((row (concat "\"" (make-string xpm-named-color-swatch-size ?a) "\""))
        (side (number-to-string xpm-named-color-swatch-size)))
    (concat "/* XPM */\nstatic char *swatch[] = {\n\""
            side " " side " 1 1\",\n"
            "\"a c " value "\",\n"
            (mapconcat #'identity
                       (make-list xpm-named-color-swatch-size row) ",\n")
            "\n};\n")))

(defun xpm-named-color-swatch (value &rest properties)
  "One swatch image for VALUE, with any image PROPERTIES applied."
  (apply #'create-image (xpm-named-color-xpm value) 'xpm t
         :ascent 'center properties))

(defun xpm-named-colors-insert (label image)
  "Insert LABEL and IMAGE as one row.
IMAGE is an image object or a string that displays one; `insert' takes only
the latter, which is what `insert-image' builds."
  (insert (propertize (format "%-14s" label) 'face xpm-named-colors-label-face))
  (if (stringp image)
      (insert image)
    (insert-image image))
  (insert "\n"))

(defun xpm-named-colors-probe ()
  (let ((buffer (get-buffer-create "*xpm-named-colors*")))
    (switch-to-buffer buffer)
    (erase-buffer)
    (setq-local cursor-type nil)
    (setq-local mode-line-format nil)
    (setq-local header-line-format nil)
    (modify-frame-parameters
     nil `((foreground-color . ,xpm-named-colors-frame-foreground)))
    (set-face-attribute 'default nil :foreground xpm-named-colors-frame-foreground)
    ;; Issue #556: the buffer's `default' face is remapped to a third colour.
    ;; The fallback swatch below is displayed under it and must still paint the
    ;; frame's foreground: GNU's FRAME_FOREGROUND_PIXEL is the frame's
    ;; foreground-color, which face remapping does not move.
    (setq-local face-remapping-alist
                `((default (:foreground ,xpm-named-colors-remap-foreground))))
    (dolist (case xpm-named-color-cases)
      (xpm-named-colors-insert case (xpm-named-color-swatch case)))
    ;; The decoys: the same unresolvable key, once under a face asking for red
    ;; and once with a specification asking for blue. Neither may win.
    (xpm-named-colors-insert
     "fallback-face"
     (propertize " " 'display
                 (xpm-named-color-swatch xpm-named-color-fallback-value)
                 'face '(:foreground "red")))
    (xpm-named-colors-insert
     "fallback-spec"
     (xpm-named-color-swatch xpm-named-color-fallback-value :foreground "blue"))
    ;; The third decoy: displayed under the buffer's remapped `default' face
    ;; (issue #556).
    (xpm-named-colors-insert
     "fallback-remap"
     (xpm-named-color-swatch xpm-named-color-fallback-value))
    (goto-char (point-min))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert
       (json-encode
        (append
         (list (cons "native-engine"
                     (if (fboundp 'neomacs--write-frame-snapshot) t :json-false))
               (cons "graphic" (if (display-graphic-p) t :json-false))
               ;; The fallback's input as this editor reports it: the frame's
               ;; foreground-color, and the default face's foreground, which GNU
               ;; keeps equal to it.
               (cons "frame-foreground"
                     (color-values (frame-parameter nil 'foreground-color)))
               (cons "default-face-foreground"
                     (color-values (face-attribute 'default :foreground)))
               (cons "face-remap-foreground"
                     (color-values xpm-named-colors-remap-foreground)))
         ;; GNU answers 16-bit channels; the Rust side reduces them like the
         ;; renderer does.
         (mapcar (lambda (case) (cons case (color-values case)))
                 xpm-named-color-cases)))))
    (redisplay t)
    ;; Let the compositor present the painted buffer before the process is
    ;; asked to exit; `kill-emacs' during startup hangs under GTK. The surface
    ;; readback is what the Rust side asserts on, so no frame snapshot is
    ;; written here.
    (run-at-time 2 nil (lambda () (kill-emacs 0)))))

(condition-case err
    (xpm-named-colors-probe)
  (error
   (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
     (insert (prin1-to-string err)))
   (kill-emacs 1)))
