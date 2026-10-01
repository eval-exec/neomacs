;;; native-scroll-contract.el --- Native rich viewport contract -*- lexical-binding: t -*-
;; Exercises canonical commands and native rendering, not OS device transport.
(require 'json)
(require 'pixel-scroll)
(load (expand-file-name "../../neomacs-perf/fixtures/scrolling-content.el"
                        (file-name-directory load-file-name)) nil t)

(defvar neomacs-native-scroll-origin nil)
(defvar neomacs-native-scroll-down nil)
(defvar neomacs-native-scroll-page nil)
(defvar neomacs-native-scroll-control (getenv "NEOMACS_GUI_SCROLL_CONTROL"))
(defun neomacs-native-scroll-position ()
  (list (window-start) (window-vscroll nil t)))
(defun neomacs-native-scroll-run (step)
  (condition-case err
      ;; Timer callbacks retain their caller's buffer, which can be *scratch*.
      ;; Canonical scroll commands must operate on the displayed buffer.
      (with-current-buffer (window-buffer (selected-window))
        (funcall step))
    (error (message "Native scroll contract: %S" err) (kill-emacs 1))))

(defun neomacs-native-scroll-next (step)
  (run-at-time 0.15 nil #'neomacs-native-scroll-run step))

(defun neomacs-native-scroll-await (stage step deadline)
  (cond
   ((file-exists-p (expand-file-name "stop" neomacs-native-scroll-control))
    (error "Rendering observer stopped at %s" stage))
   ((file-exists-p (expand-file-name (concat stage ".ack") neomacs-native-scroll-control))
    (funcall step))
   ((> (float-time) deadline)
    (error "Timed out awaiting rendered %s viewport" stage))
   (t (run-at-time
       0.05 nil #'neomacs-native-scroll-run
       (lambda () (neomacs-native-scroll-await stage step deadline))))))

(defun neomacs-native-scroll-present (stage step)
  "Hold STAGE until the optional external rendering observer acknowledges it."
  (if (not neomacs-native-scroll-control)
      (funcall step)
    ;; Reading dimensions and Lisp positions must not run a frame snapshot:
    ;; that observer can itself replace the viewport geometry under test.
    (let ((position (neomacs-native-scroll-position))
          (width (frame-pixel-width))
          (height (frame-pixel-height)))
      (with-temp-file (expand-file-name (concat stage ".ready")
                                       neomacs-native-scroll-control)
        (insert (json-encode `((stage . ,stage) (position . ,position)
                               (width . ,width) (height . ,height))) "\n")))
    (neomacs-native-scroll-await stage step (+ (float-time) 20))))

(defun neomacs-native-scroll-pixel-down ()
  (setq neomacs-native-scroll-origin (neomacs-native-scroll-position))
  (neomacs-native-scroll-present
   "origin"
   (lambda ()
     (pixel-scroll-precision-scroll-down 7)
     (neomacs-native-scroll-next #'neomacs-native-scroll-pixel-up))))
(defun neomacs-native-scroll-pixel-up ()
  (setq neomacs-native-scroll-down (neomacs-native-scroll-position))
  (unless (and (= (car neomacs-native-scroll-down) (car neomacs-native-scroll-origin))
               (= (- (cadr neomacs-native-scroll-down)
                     (cadr neomacs-native-scroll-origin)) 7))
    (error "Expected exactly seven pixels: %S -> %S"
           neomacs-native-scroll-origin neomacs-native-scroll-down))
  (neomacs-native-scroll-present
   "down"
   (lambda ()
     (pixel-scroll-precision-scroll-up 7)
     (neomacs-native-scroll-next #'neomacs-native-scroll-page-down))))
(defun neomacs-native-scroll-page-down ()
  (unless (equal (neomacs-native-scroll-position) neomacs-native-scroll-origin)
    (error "Opposite pixel scroll did not return to its initial viewport"))
  (scroll-up-command)
  (neomacs-native-scroll-next #'neomacs-native-scroll-page-up))
(defun neomacs-native-scroll-page-up ()
  (setq neomacs-native-scroll-page (window-start))
  (unless (> neomacs-native-scroll-page (car neomacs-native-scroll-origin))
    (error "PageDown did not advance the rich viewport"))
  (scroll-down-command)
  (neomacs-native-scroll-next #'neomacs-native-scroll-finish))
(defun neomacs-native-scroll-finish ()
  (unless (< (window-start) neomacs-native-scroll-page)
    (error "PageUp did not return towards the initial viewport"))
  ;; Capture buffer-local metadata before with-temp-file selects its buffer.
  (let ((content neomacs-scroll-content-summary)
        (start (window-start)))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode
               `((contract . "native-scroll")
                 (content . ,content) (final-start . ,start)
                 (pixel-origin . ,neomacs-native-scroll-origin)
                 (pixel-down . ,neomacs-native-scroll-down)
                 (pixel-returned . t) (page-advanced . t) (page-returned . t))) "\n")))
  (neomacs--write-frame-snapshot
   (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json-geometry)
  (run-at-time 0.2 nil #'kill-emacs 0))

(neomacs-native-scroll-next
 (lambda ()
   (switch-to-buffer (get-buffer-create "*native-scroll-contract*"))
   (neomacs-scroll-content-insert 100000)
   (goto-char (point-min))
   (forward-line 50000)
   (set-window-start nil (point) t)
   (neomacs-native-scroll-next #'neomacs-native-scroll-pixel-down)))
