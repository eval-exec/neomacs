;;; native-scroll-contract.el --- Native rich viewport contract -*- lexical-binding: t -*-
;; Exercises canonical commands and native rendering, not OS device transport.
(require 'json)
(require 'pixel-scroll)
(load (expand-file-name "../../neomacs-perf/fixtures/scrolling-content.el"
                        (file-name-directory load-file-name)) nil t)

(defvar neomacs-native-scroll-origin nil)
(defvar neomacs-native-scroll-page nil)
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

(defun neomacs-native-scroll-pixel-down ()
  (setq neomacs-native-scroll-origin (neomacs-native-scroll-position))
  (pixel-scroll-precision-scroll-down 7)
  (neomacs-native-scroll-next #'neomacs-native-scroll-pixel-up))
(defun neomacs-native-scroll-pixel-up ()
  (when (equal (neomacs-native-scroll-position) neomacs-native-scroll-origin)
    (error "Seven-pixel scroll did not move the rich viewport"))
  (pixel-scroll-precision-scroll-up 7)
  (neomacs-native-scroll-next #'neomacs-native-scroll-page-down))
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
                 (pixel-returned . t) (page-advanced . t) (page-returned . t))) "\n")))
  (neomacs--write-frame-snapshot
   (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
  (run-at-time 0.2 nil #'kill-emacs 0))

(neomacs-native-scroll-next
 (lambda ()
   (switch-to-buffer (get-buffer-create "*native-scroll-contract*"))
   (neomacs-scroll-content-insert 100000)
   (goto-char (point-min))
   (forward-line 50000)
   (set-window-start nil (point) t)
   (neomacs-native-scroll-next #'neomacs-native-scroll-pixel-down)))
