;;; encoded-file.el --- File decoding survives native GUI input -*- lexical-binding: t; -*-
(require 'json)
(setq inhibit-startup-screen t
      frame-title-format "neomacs-encoded-file")
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(defvar neo-encoded-file-control (getenv "NEOMACS_ENCODED_FILE_CONTROL"))
(defvar neo-encoded-file-fixtures (getenv "NEOMACS_ENCODED_FILE_FIXTURES"))

(defun neo-encoded-file-await-presentation ()
  (if (file-exists-p (expand-file-name "painted" neo-encoded-file-control))
      (kill-emacs 0)
    (run-at-time 0.05 nil #'neo-encoded-file-await-presentation)))

(defun neo-encoded-file-fail (err)
  (with-temp-file (expand-file-name "result.json" neo-encoded-file-control)
    (insert (json-encode `((passed . :json-false) (error . ,(format "%S" err))))))
  (kill-emacs 1))

(defun neo-encoded-file-after-input ()
  (when (and (equal (buffer-name) "*encoded file*")
             (equal (buffer-string) "x"))
    (remove-hook 'post-command-hook #'neo-encoded-file-after-input)
    (insert "\nENCODED-FILE-PASSED\n")
    (goto-char (point-min))
    (set-face-attribute 'default nil :foreground "#000000" :background "#00ff00")
    (redisplay t)
    (neomacs--write-frame-snapshot
     (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") nil 'text)
    (neomacs--write-frame-snapshot
     (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
    (with-temp-file (expand-file-name "result.json" neo-encoded-file-control)
      (insert (json-encode '((passed . t) (typed . "x")))))
    (neo-encoded-file-await-presentation)))

(defun neo-encoded-file-start ()
  (condition-case err
      (progn
        (unless (display-graphic-p) (error "Expected GUI frame"))
        (load (expand-file-name "valid.elc" neo-encoded-file-fixtures) nil t t)
        (unless (equal (list neo-encoded-file-result neo-encoded-file-malformed
                             neo-encoded-file-after)
                       '((233 30333 128512 2097152 4194175 4194176 4194303)
                         (195 40) loaded))
          (error "Compiled-file character identity changed"))
        ;; This recovery policy is Neo-only: GNU's unchecked decoder assumes
        ;; that the five-byte payload is in range.
        (load (expand-file-name "oversized.elc" neo-encoded-file-fixtures) nil t t)
        (unless (and (= (car neo-encoded-file-oversized) #x3ffff8)
                     (> (length neo-encoded-file-oversized) 1)
                     (eq neo-encoded-file-after 'loaded))
          (error "Oversized file lost its lead byte or following form"))
        (dolist (code neo-encoded-file-oversized)
          (unless (<= 0 code #x3fffff) (error "Out of character domain")))
        (switch-to-buffer (get-buffer-create "*encoded file*"))
        (erase-buffer)
        (setq-local mode-line-format '(" Encoded file "))
        (add-hook 'post-command-hook #'neo-encoded-file-after-input)
        (redisplay t)
        (with-temp-file (expand-file-name "ready" neo-encoded-file-control)
          (insert (number-to-string (emacs-pid)))))
    (error (neo-encoded-file-fail err))))
(run-at-time 0.5 nil #'neo-encoded-file-start)
(run-at-time 35 nil (lambda () (kill-emacs 2)))
