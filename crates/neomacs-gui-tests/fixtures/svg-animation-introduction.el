;;; svg-animation-introduction.el --- SVG introduction playback -*- lexical-binding: t -*-

(require 'image)
(require 'json)
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)
(set-face-attribute 'default nil :foreground "#000000" :background "#ffffff")
(set-frame-size nil 500 400 t)

(defvar neomacs-svg-control (getenv "NEOMACS_GUI_SVG_ANIMATION_CONTROL"))
(defvar neomacs-svg-image nil)
(defvar neomacs-svg-playback nil)
(defvar neomacs-svg-deadline nil)

;; Observe the real image.el callbacks without replacing playback behavior.
(defun neomacs-svg-record-frame (image index &rest _)
  (when (eq image neomacs-svg-image) (push index neomacs-svg-playback)))

(defun neomacs-svg-intro-capture (stage)
  (let ((multi (image-multi-frame-p neomacs-svg-image))
        (metadata (image-metadata neomacs-svg-image)))
    (with-temp-file (expand-file-name (concat stage "-state.json") neomacs-svg-control)
      (insert (json-encode
               (vector `((count . ,(car multi)) (delay . ,(cdr multi))
                         (index . ,(image-current-frame neomacs-svg-image))
                         (loop-start . ,(plist-get metadata 'loop-start))))))))
  (neomacs--write-frame-snapshot
   (expand-file-name (concat stage ".json") neomacs-svg-control) nil 'json)
  (with-temp-file (expand-file-name (concat stage ".ready") neomacs-svg-control)
    (insert "ready"))
  (run-at-time 0.05 nil #'neomacs-svg-intro-await-presentation stage))

(defun neomacs-svg-intro-await-presentation (stage)
  (cond
   ((file-exists-p (expand-file-name "stop" neomacs-svg-control)) (kill-emacs 2))
   ((file-exists-p (expand-file-name (concat stage ".ack") neomacs-svg-control))
    (if (equal stage "final") (kill-emacs 0)
      (advice-add 'image-show-frame :before #'neomacs-svg-record-frame)
      (image-animate neomacs-svg-image nil t)
      (setq neomacs-svg-deadline (+ (float-time) 12))
      (run-at-time 0.05 nil #'neomacs-svg-intro-await-cycle)))
   (t (run-at-time 0.05 nil #'neomacs-svg-intro-await-presentation stage))))

(defun neomacs-svg-intro-await-cycle ()
  (cond
   ((> (float-time) neomacs-svg-deadline)
    (message "Delayed SVG did not finish its first steady cycle") (kill-emacs 1))
   ((>= (length neomacs-svg-playback) 5)
    (let ((timer (image-animate-timer neomacs-svg-image)))
      (when timer (cancel-timer timer)))
    (advice-remove 'image-show-frame #'neomacs-svg-record-frame)
    (with-temp-file (expand-file-name "playback.json" neomacs-svg-control)
      (insert (json-encode (vconcat (reverse neomacs-svg-playback)))))
    (neomacs-svg-intro-capture "final"))
   (t (run-at-time 0.05 nil #'neomacs-svg-intro-await-cycle))))

(defun neomacs-svg-intro-await-metadata ()
  (condition-case err
      (cond
       ((> (float-time) neomacs-svg-deadline) (error "Delayed SVG metadata absent"))
       ((image-multi-frame-p neomacs-svg-image) (neomacs-svg-intro-capture "initial"))
       (t (redisplay t) (run-at-time 0.05 nil #'neomacs-svg-intro-await-metadata)))
    (error (message "Delayed SVG metadata: %S" err) (kill-emacs 1))))

(defun neomacs-svg-intro-start ()
  (condition-case err
      (progn
        (switch-to-buffer (get-buffer-create "*svg-introduction*"))
        (delete-other-windows)
        (setq neomacs-svg-image
              (create-image "<svg xmlns='http://www.w3.org/2000/svg' width='80' height='80'><rect width='80' height='80' fill='#ff0000'><animate attributeName='fill' from='#0000ff' to='#00ffff' begin='1s' dur='1s' repeatCount='indefinite'/></rect></svg>"
                            'svg t :animation 2 :scale 1))
        (insert-image neomacs-svg-image)
        (insert "\n")
        (insert-image (create-image "<svg xmlns='http://www.w3.org/2000/svg' width='80' height='80'><rect width='80' height='80' fill='#0000ff'/></svg>"
                                    'svg t :scale 1))
        (goto-char (point-min))
        (setq neomacs-svg-deadline (+ (float-time) 12))
        (redisplay t)
        (neomacs-svg-intro-await-metadata))
    (error (message "Delayed SVG setup: %S" err) (kill-emacs 1))))

(run-at-time 0.5 nil #'neomacs-svg-intro-start)
