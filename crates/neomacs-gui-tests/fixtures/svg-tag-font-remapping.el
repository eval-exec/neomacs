;;; svg-tag-font-remapping.el --- SVG tag measurement regression -*- lexical-binding: t -*-

(require 'face-remap)
(dolist (variable '("NEOMACS_GUI_SVG_LIB_DIR" "NEOMACS_GUI_SVG_TAG_MODE_DIR"))
  (let ((directory (getenv variable)))
    (unless directory (error "Set %s to the installed package directory" variable))
    (add-to-list 'load-path directory)))
(require 'svg-tag-mode)

(defun neomacs-svg-tag-metrics ()
  (let* ((image (svg-tag-make "TODO" :margin 0 :radius 8 :padding 0.07
                              :height 0.71 :ascent 79
                              :font-family "DejaVu Sans Mono" :font-size 7))
         (data (plist-get (cdr image) :data)))
    (unless (string-match "<svg width=\"\\([^\"]+\\)\" height=\"\\([^\"]+\\)\"" data)
      (error "Missing SVG dimensions: %s" data))
    (let ((width (string-to-number (match-string 1 data)))
          (height (string-to-number (match-string 2 data))))
      (unless (string-match "font-size=\"\\([^\"]+\\)\"" data)
        (error "Missing SVG text size: %s" data))
      (let ((text-size (string-to-number (match-string 1 data))))
        (list (window-font-width) (window-font-height) width height text-size)))))

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (switch-to-buffer (get-buffer-create "*svg-tag-font-remapping*"))
         (set-face-attribute 'default nil :font
                             (font-spec :family "DejaVu Sans Mono" :size 25))
         (insert "TODO: SVG tag box follows the remapped window font.\n")
         (redisplay t)
         (let ((base (neomacs-svg-tag-metrics))
               (frame-width (frame-char-width))
               (frame-height (frame-char-height)))
           (face-remap-add-relative 'default :height 1.2)
           (redisplay t)
           (let ((scaled (neomacs-svg-tag-metrics)))
             (message "SVG remapping metrics: base=%S scaled=%S" base scaled)
             ;; GNU: remapping enlarges the window font and generated box,
             ;; without enlarging either the frame's canonical cells or
             ;; the separately specified SVG text font.
             (dotimes (index 4)
               (unless (> (nth index scaled) (nth index base))
                 (error "Remapping did not enlarge metric %d: %S -> %S" index base scaled)))
             ;; Independently captured from GNU Emacs with this 25px font:
             ;; cells 15x30 -> 18x36, matching issue #360's SVG box dimensions.
             (unless (and (= (nth 0 base) 15) (= (nth 1 base) 30)
                          (= (nth 0 scaled) 18) (= (nth 1 scaled) 36)
                          (< (abs (- (nth 2 base) 61.05)) 0.000001)
                          (< (abs (- (nth 3 base) 21.30)) 0.000001)
                          (< (abs (- (nth 2 scaled) 73.26)) 0.000001)
                          (< (abs (- (nth 3 scaled) 25.56)) 0.000001))
               (error "SVG geometry differs from GNU oracle: %S -> %S" base scaled))
             (unless (and (= (nth 4 base) (nth 4 scaled))
                          (= frame-width (frame-char-width))
                          (= frame-height (frame-char-height)))
               (error "Remapping changed the fixed SVG font or canonical frame cells"))
             (setq face-remapping-alist nil)
             (unless (equal base (neomacs-svg-tag-metrics))
               (error "Removing remapping did not restore SVG dimensions"))))
         (when (fboundp 'neomacs--write-frame-snapshot)
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
         (run-at-time 1 nil (lambda () (kill-emacs 0))))
     (error (message "SVG remapping regression: %S" err) (kill-emacs 1)))))
