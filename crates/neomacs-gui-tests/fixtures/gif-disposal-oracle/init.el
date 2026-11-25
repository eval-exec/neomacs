;;; GNU gif_load disposal, transparency, and bounds parity under a real frame. -*- lexical-binding: t; -*-

;; Runs identically under GNU Emacs 31.1 and Neomacs on a graphic frame.
;; Displays five GIF fixtures generated in a temporary directory by the test
;; harness:
;;   1. `disposal.gif' at :index 1: 80x80 frame 0 red (DISPOSE_BACKGROUND),
;;      frame 1 40x40 green at (20,20) (DISPOSE_DO_NOT). GNU gif_load
;;      (src/image.c:10185-10263) never clears disposed frames, so all 4800
;;      outer red pixels survive beside the 1600 green pixels.
;;   2. `spec-bg.gif': 48x48 transparent GIF with explicit :background "#123456"
;;      under a cyan face background decoy; :background must win.
;;   3. `frame-bg.gif': 48x48 GIF with a 4px magenta (#ff00ff) border and 40x40
;;      transparent interior, displayed without :background under a cyan face
;;      background decoy inside a buffer whose `default' face is remapped to
;;      #f5f5f5, on a frame whose `background-color' is #e8d8c8. GNU gif_load
;;      (src/image.c:10092-10101, :10215-10217) paints transparent pixels with
;;      FRAME_BACKGROUND_PIXEL (#e8d8c8), ignoring both face-remapping-alist and
;;      the displaying face's background.
;;   4. `oob-single.gif': 48x48 screen with a single 48x48 orange (#ff8000)
;;      subimage shifted to left=10 (right=58 > 48). GNU rejects the image
;;      (src/image.c:10018-10035), painting 0 orange pixels.
;;   5. `oob-frame1.gif' at :index 0: 48x48 screen with an in-bounds yellow
;;      (#ffff00) frame 0 and an out-of-bounds frame 1 at left=10. GNU checks
;;      all subimages up front and rejects the whole image for every :index,
;;      painting 0 yellow pixels even for :index 0.

(require 'image)
(require 'json)

(defconst gif-disposal-frame-bg "#e8d8c8")
(defconst gif-disposal-remap-bg "#f5f5f5")
(defconst gif-disposal-face-decoy "#00ffff")
(defconst gif-disposal-spec-bg "#123456")

(defun gif-disposal-raw-image-size (img)
  "Return the raw `image-size' outcome without conflating a GNU placeholder
with a Neomacs signal: `[\"ok\", W, H]' or `[\"error\", MSG]'."
  (condition-case err
      (let ((sz (image-size img t)))
        (vector "ok" (car sz) (cdr sz)))
    (error (vector "error" (cadr err)))))

(defun gif-disposal-probe ()
  (let* ((dir (getenv "NEOMACS_GIF_DISPOSAL_DIR"))
         (disposal-file (expand-file-name "disposal.gif" dir))
         (spec-bg-file (expand-file-name "spec-bg.gif" dir))
         (frame-bg-file (expand-file-name "frame-bg.gif" dir))
         (oob-single-file (expand-file-name "oob-single.gif" dir))
         (oob-frame1-file (expand-file-name "oob-frame1.gif" dir))
         (buf (get-buffer-create "*gif-disposal-oracle*")))
    (switch-to-buffer buf)
    (erase-buffer)
    (setq-local cursor-type nil)
    (setq-local mode-line-format nil)
    (setq-local header-line-format nil)
    (modify-frame-parameters
     nil `((background-color . ,gif-disposal-frame-bg)
           (foreground-color . "#000000")))
    (set-face-attribute 'default nil
                        :background gif-disposal-frame-bg
                        :foreground "#000000")
    ;; Remap the buffer's `default' face to a light background so the frame
    ;; background (#e8d8c8) stands out inside the magenta-framed swatch.
    (setq-local face-remapping-alist
                `((default (:background ,gif-disposal-remap-bg))))
    (let* ((disposal-img (create-image disposal-file 'gif nil :scale 1 :index 1))
           (spec-bg-img (create-image spec-bg-file 'gif nil
                                      :scale 1 :background gif-disposal-spec-bg))
           (frame-bg-img (create-image frame-bg-file 'gif nil :scale 1))
           (frame-bg-heuristic (create-image frame-bg-file 'gif nil
                                             :scale 1 :mask 'heuristic))
           (oob-single-img (create-image oob-single-file 'gif nil :scale 1 :index 0))
           (oob-frame1-0-img (create-image oob-frame1-file 'gif nil :scale 1 :index 0))
           (oob-frame1-1-img (create-image oob-frame1-file 'gif nil :scale 1 :index 1)))
      (insert (propertize " " 'display disposal-img) "\n\n")
      (insert (propertize " " 'display spec-bg-img
                          'face `(:background ,gif-disposal-face-decoy))
              "\n\n")
      (insert (propertize " " 'display frame-bg-img
                          'face `(:background ,gif-disposal-face-decoy))
              "\n\n")
      (insert (propertize " " 'display oob-single-img)
              " "
              (propertize " " 'display oob-frame1-0-img)
              "\n")
      (goto-char (point-min))
      (redisplay t)
      (let ((disposal-size (image-size disposal-img t))
            (mask-default (and (image-mask-p frame-bg-img) t))
            (mask-heuristic (and (image-mask-p frame-bg-heuristic) t))
            (oob-single-size (gif-disposal-raw-image-size oob-single-img))
            (oob-frame1-0-size (gif-disposal-raw-image-size oob-frame1-0-img))
            (oob-frame1-1-size (gif-disposal-raw-image-size oob-frame1-1-img))
            (oob-single-meta (and (image-metadata oob-single-img) t))
            (oob-frame1-0-meta (and (image-metadata oob-frame1-0-img) t))
            (oob-frame1-1-meta (and (image-metadata oob-frame1-1-img) t)))
        (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
          (insert
           (json-encode
            (list (cons "native-engine"
                        (if (fboundp 'neomacs--write-frame-snapshot) t :json-false))
                  (cons "graphic" (if (display-graphic-p) t :json-false))
                  (cons "disposal-width" (car disposal-size))
                  (cons "disposal-height" (cdr disposal-size))
                  (cons "mask-default" (if mask-default t :json-false))
                  (cons "mask-heuristic" (if mask-heuristic t :json-false))
                  (cons "oob-single-size" oob-single-size)
                  (cons "oob-frame1-0-size" oob-frame1-0-size)
                  (cons "oob-frame1-1-size" oob-frame1-1-size)
                  (cons "oob-single-meta" (if oob-single-meta t :json-false))
                  (cons "oob-frame1-0-meta" (if oob-frame1-0-meta t :json-false))
                  (cons "oob-frame1-1-meta" (if oob-frame1-1-meta t :json-false))
                  (cons "frame-background"
                        (color-values (frame-parameter nil 'background-color)))))))))
    (redisplay t)
    (run-at-time
     0.5 nil
     (lambda ()
       (redisplay t)
       (when (getenv "NEOMACS_GNU_EXPORT_PNG")
         (let ((coding-system-for-write 'no-conversion))
           (with-temp-file (getenv "NEOMACS_GNU_EXPORT_PNG")
             (set-buffer-multibyte nil)
             (insert (x-export-frames nil 'png)))))))
    (run-at-time 2 nil (lambda () (kill-emacs 0)))))

(condition-case err
    (gif-disposal-probe)
  (error
   (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
     (insert (prin1-to-string err)))
   (kill-emacs 1)))
