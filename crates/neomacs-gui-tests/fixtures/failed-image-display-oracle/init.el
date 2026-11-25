;;; init.el --- Failed-image display geometry & hollow-box oracle -*- lexical-binding: t; -*-

(require 'json)

(setq inhibit-startup-screen t)
(setq initial-scratch-message nil)
(setq ring-bell-function #'ignore)
(setq-default cursor-type nil)
(when (fboundp 'blink-cursor-mode) (blink-cursor-mode -1))
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
(when (fboundp 'scroll-bar-mode) (scroll-bar-mode -1))
(when (fboundp 'fringe-mode) (fringe-mode 0))

(defconst failed-box-frame-bg "#102030")
(defconst failed-box-frame-fg "#eeeeee")

(defun failed-box-probe ()
  (let* ((dir (getenv "NEOMACS_FAILED_IMAGE_DIR"))
         (oob-gif (expand-file-name "oob-48x48.gif" dir))
         (missing-img (create-image "/nonexistent/failed-box-missing.png" 'png))
         (margined-img (create-image "/nonexistent/failed-box-margined.png" 'png nil
                                     :margin 10 :relief 4 :ascent 80))
         (sized-img (create-image "not-a-png" 'png t :width 40 :height 36))
         (oob-img (create-image oob-gif 'gif nil :scale 1))
         (buf (get-buffer-create "*failed-image-display-oracle*")))
    (set-frame-parameter nil 'background-color failed-box-frame-bg)
    (set-frame-parameter nil 'foreground-color failed-box-frame-fg)
    (set-face-attribute 'default nil
                        :background failed-box-frame-bg
                        :foreground failed-box-frame-fg)
    (switch-to-buffer buf)
    (erase-buffer)
    (setq-local mode-line-format nil)
    (setq-local header-line-format nil)
    (setq-local cursor-type nil)
    (setq-local truncate-lines t)
    ;; Four failed-image rows with distinct foreground (hollow box outline)
    ;; and background (interior fill) colors, each followed by a single marker char.
    (insert (propertize "A" 'display missing-img
                        'face '(:foreground "#ff0000" :background "#00ff00"))
            "X\n"
            (propertize "B" 'display margined-img
                        'face '(:foreground "#0000ff" :background "#ffff00"))
            "Y\n"
            (propertize "C" 'display sized-img
                        'face '(:foreground "#ff00ff" :background "#00ffff"))
            "Z\n"
            (propertize "D" 'display oob-img
                        'face '(:foreground "#ff8000" :background "#0080ff"))
            "W\n")
    (goto-char (point-max))
    ;; Query image-size synchronously first so the failed decode settles on both editors.
    (let ((sz-missing (image-size missing-img t))
          (sz-margined (image-size margined-img t))
          (sz-sized (image-size sized-img t))
          (sz-oob (image-size oob-img t)))
      (redisplay t)
      (run-at-time
       0.5 nil
       (lambda ()
         (redisplay t)
         (let* ((wtps-missing (window-text-pixel-size nil 1 2))
                (wtps-margined (window-text-pixel-size nil 4 5))
                (wtps-sized (window-text-pixel-size nil 7 8))
                (wtps-oob (window-text-pixel-size nil 10 11))
                (p1 (posn-x-y (posn-at-point 1)))
                (p2 (posn-x-y (posn-at-point 2)))
                (p4 (posn-x-y (posn-at-point 4)))
                (p5 (posn-x-y (posn-at-point 5)))
                (p7 (posn-x-y (posn-at-point 7)))
                (p8 (posn-x-y (posn-at-point 8)))
                (p10 (posn-x-y (posn-at-point 10)))
                (p11 (posn-x-y (posn-at-point 11))))
           (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
             (insert
              (json-encode
               (list
                (cons "native-engine"
                      (if (fboundp 'neomacs--write-frame-snapshot) t :json-false))
                (cons "graphic" (if (display-graphic-p) t :json-false))
                (cons "sz-missing" (list (car sz-missing) (cdr sz-missing)))
                (cons "sz-margined" (list (car sz-margined) (cdr sz-margined)))
                (cons "sz-sized" (list (car sz-sized) (cdr sz-sized)))
                (cons "sz-oob" (list (car sz-oob) (cdr sz-oob)))
                (cons "wtps-missing" (list (car wtps-missing) (cdr wtps-missing)))
                (cons "wtps-margined" (list (car wtps-margined) (cdr wtps-margined)))
                (cons "wtps-sized" (list (car wtps-sized) (cdr wtps-sized)))
                (cons "wtps-oob" (list (car wtps-oob) (cdr wtps-oob)))
                (cons "adv-missing" (- (car p2) (car p1)))
                (cons "adv-margined" (- (car p5) (car p4)))
                (cons "adv-sized" (- (car p8) (car p7)))
                (cons "adv-oob" (- (car p11) (car p10)))))))
           (when (getenv "NEOMACS_GNU_EXPORT_PNG")
             (let ((coding-system-for-write 'no-conversion))
               (with-temp-file (getenv "NEOMACS_GNU_EXPORT_PNG")
                 (set-buffer-multibyte nil)
                 (insert (x-export-frames nil 'png)))))))))
    (run-at-time 1.8 nil (lambda () (kill-emacs 0)))))

(condition-case err
    (failed-box-probe)
  (error
   (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
     (insert (prin1-to-string err)))
   (kill-emacs 1)))
