;;; close-confirmation.el --- Keep close decisions in Lisp -*- lexical-binding: t -*-
(require 'json)
(setq inhibit-startup-screen t use-dialog-box t frame-title-format "%F")
(modify-frame-parameters nil '((name . "CLOSE-TEST-PRIMARY") (title . "CLOSE-TEST-PRIMARY")))
(defvar close-test-directory (getenv "NEOMACS_GUI_CLOSE_CONTROL"))
(when (equal (getenv "NEOMACS_GUI_CLOSE_MODE") "modified")
  (find-file (expand-file-name "unsaved.txt" close-test-directory))
  (insert "unsaved change"))
(defvar close-test-protected-frame nil)
(when (equal (getenv "NEOMACS_GUI_CLOSE_MODE") "protected-secondary")
  (setq close-test-protected-frame
        (make-frame '((name . "CLOSE-TEST-SECONDARY") (title . "CLOSE-TEST-SECONDARY"))))
  (select-frame-set-input-focus close-test-protected-frame)
  (define-key special-event-map [delete-frame]
    (lambda (_event)
      (interactive "e")
      (with-temp-file (expand-file-name "close-handled" close-test-directory)
        (insert "Lisp declined deletion")))))

(defun close-test-observe ()
  (interactive)
  (let ((state `((protected-live . ,(and close-test-protected-frame
                                       (frame-live-p close-test-protected-frame) t))
                 (modified . ,(buffer-modified-p))
                 (text . ,(buffer-string)))))
    (with-temp-file (expand-file-name "responsive" close-test-directory)
      (insert (json-encode state)))))
(global-set-key (kbd "C-c t") #'close-test-observe)
(run-at-time 1 nil
             (lambda ()
               (with-temp-file (expand-file-name "ready" close-test-directory)
                 (insert "ready"))))
