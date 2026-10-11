;;; svg-animation.el --- Animated SVG presentation regression -*- lexical-binding: t -*-

(require 'image)
(require 'json)
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)
(set-face-attribute 'default nil :foreground "#000000" :background "#ffffff")
(set-frame-size nil 500 400 t)

(defvar neomacs-svg-control (getenv "NEOMACS_GUI_SVG_ANIMATION_CONTROL"))
(defvar neomacs-svg-images nil)
(defvar neomacs-svg-deadline nil)

(defun neomacs-svg-every (predicate values)
  (catch 'not-ready
    (dolist (value values t)
      (unless (funcall predicate value) (throw 'not-ready nil)))))

(defun neomacs-svg-capture (stage)
  (with-temp-file (expand-file-name (concat stage "-state.json") neomacs-svg-control)
    (insert
     (json-encode
      (vconcat
       (mapcar (lambda (image)
                 (let ((multi (image-multi-frame-p image)))
                   `((count . ,(car multi)) (delay . ,(cdr multi))
                     (index . ,(image-current-frame image)))))
               neomacs-svg-images)))))
  (neomacs--write-frame-snapshot
   (expand-file-name (concat stage ".json") neomacs-svg-control) nil 'json)
  (with-temp-file (expand-file-name (concat stage ".ready") neomacs-svg-control)
    (insert "ready"))
  (run-at-time 0.05 nil #'neomacs-svg-await-presentation stage))

(defun neomacs-svg-await-presentation (stage)
  (cond
   ((file-exists-p (expand-file-name "stop" neomacs-svg-control)) (kill-emacs 2))
   ((file-exists-p (expand-file-name (concat stage ".ack") neomacs-svg-control))
    (if (equal stage "final")
        (kill-emacs 0)
      ;; Metadata and the initial GPU presentation must arrive before the
      ;; ordinary image.el timers are allowed to change either image.
      (mapc #'image-animate neomacs-svg-images)
      (setq neomacs-svg-deadline (+ (float-time) 12))
      (run-at-time 0.05 nil #'neomacs-svg-await-final)))
   (t (run-at-time 0.05 nil #'neomacs-svg-await-presentation stage))))

(defun neomacs-svg-await-final ()
  (cond
   ((> (float-time) neomacs-svg-deadline)
    (message "SVG animations failed to stop on their final frames") (kill-emacs 1))
   ((neomacs-svg-every (lambda (image)
                (and (= (image-current-frame image) 2)
                     (not (image-animate-timer image))))
              neomacs-svg-images)
    (neomacs-svg-capture "final"))
   (t (run-at-time 0.05 nil #'neomacs-svg-await-final))))

(defun neomacs-svg-await-metadata ()
  (condition-case err
      (cond
       ((> (float-time) neomacs-svg-deadline)
        (error "Animated SVG metadata did not arrive"))
       ((neomacs-svg-every #'image-multi-frame-p neomacs-svg-images)
        (neomacs-svg-capture "initial"))
       (t (redisplay t) (run-at-time 0.05 nil #'neomacs-svg-await-metadata)))
    (error (message "SVG metadata regression: %S" err) (kill-emacs 1))))

(defun neomacs-svg-start ()
  (condition-case err
      (let* ((plain "<svg xmlns='http://www.w3.org/2000/svg' width='80' height='80'><rect width='80' height='80' fill='#ff0000'><animate attributeName='fill' from='#ff0000' to='#0000ff' dur='1s' fill='freeze'/></rect></svg>")
             ;; gzip of an SVG using s:svg, s:rect, and s:animate. Its
             ;; green first frame proves both decompression and namespace
             ;; detection reached the animated sampling path.
             (compressed (base64-decode-string "H4sIAAAAAAAC/3WOQQrCMBBFrzKM+2bUjZSmR/AO0U6aQNNKMjXi6U1TxJW79/mPz+9Sm54jvMI0F9LoRB6tUjnnJp+bJY7qRESqOAjZD+I0XgjBsR+dVO671Ea+y58arJ8mjQcia2m3zeyDEQYjEv1tFb6awBo3sehxCT8dZKlhiwjDGjUe03fTRuY3o+o7tV+oUK72H3WOPenWAAAA"))
             (expected "<svg xmlns='http://www.w3.org/2000/svg' width='80' height='80'><rect width='80' height='80' fill='#0000ff'/></svg>"))
        (switch-to-buffer (get-buffer-create "*svg-animation*"))
        (delete-other-windows)
        (setq neomacs-svg-images
              (list (create-image plain 'svg t :animation 2 :scale 1)
                    (create-image compressed 'svg t :animation 2 :scale 1)))
        (dolist (image neomacs-svg-images)
          (insert-image image)
          (insert "\n"))
        (insert-image (create-image expected 'svg t :animation nil :index 19 :scale 1))
        (goto-char (point-min))
        (setq neomacs-svg-deadline (+ (float-time) 12))
        (redisplay t)
        (neomacs-svg-await-metadata))
    (error (message "SVG animation fixture failed: %S" err) (kill-emacs 1))))

(run-at-time 0.5 nil #'neomacs-svg-start)
