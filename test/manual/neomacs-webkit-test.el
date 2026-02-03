;;; neomacs-webkit-test.el --- Test inline WebKit rendering in Neomacs -*- lexical-binding: t -*-

;; This test verifies that WebKit views are rendered inline in buffers
;; using WPE WebKit with GPU acceleration.

;;; Commentary:
;; Run with: ./test/manual/run-webkit-test.sh
;; Or manually: DISPLAY=:0 ./src/emacs -Q -l test/manual/neomacs-webkit-test.el
;;
;; Neomacs renders WebKit views inline in buffers (not as floating overlays).
;; This follows the standard Emacs xwidget pattern:
;;
;;   (insert (propertize " " 'display (neomacs-insert-webkit url width height t)))
;;
;; The webkit view becomes part of the buffer content, scrolls naturally,
;; and respects Emacs window management.

;;; Code:

(defvar neomacs-webkit-test-url "https://youtube.com"
  "URL to load for testing.")

(defvar neomacs-webkit-test-width 800
  "Width of test WebKit view.")

(defvar neomacs-webkit-test-height 600
  "Height of test WebKit view.")

(defun neomacs-webkit-test-run ()
  "Run the inline WebKit rendering test."
  (interactive)
  (switch-to-buffer (get-buffer-create "*WebKit Test*"))
  (erase-buffer)

  (insert "=== Neomacs Inline WebKit Test ===\n\n")

  (condition-case err
      (progn
        (insert "Initializing WebKit subsystem...\n")
        (neomacs-webkit-init)
        (insert "WebKit initialized.\n\n")

        (insert (format "Loading %s inline...\n\n" neomacs-webkit-test-url))

        ;; Create inline webkit view
        (let ((spec (neomacs-insert-webkit neomacs-webkit-test-url
                                           neomacs-webkit-test-width
                                           neomacs-webkit-test-height
                                           t)))
          (if spec
              (progn
                ;; Insert the webkit view inline
                (insert (propertize " " 'display spec))
                (insert "\n\n")
                (insert (format "WebKit spec: %S\n\n" spec))
                (insert "SUCCESS! Inline WebKit rendering works.\n")
                (insert "\nControls (use view ID from spec above):\n")
                (insert "  (neomacs-webkit-load-uri ID \"url\") - load new URL\n")
                (insert "  (neomacs-webkit-go-back ID) - go back\n")
                (insert "  (neomacs-webkit-go-forward ID) - go forward\n")
                (insert "  (neomacs-webkit-reload ID) - reload\n"))
            (insert "FAILED: neomacs-insert-webkit returned nil\n"))))
    (error
     (insert (format "ERROR: %S\n" err))))

  (goto-char (point-min))
  (redisplay t)

  ;; Auto-exit after delay when run non-interactively
  (when noninteractive
    (run-at-time 15 nil (lambda () (kill-emacs 0)))))

;; Auto-run when loaded
(add-hook 'emacs-startup-hook #'neomacs-webkit-test-run)

(provide 'neomacs-webkit-test)
;;; neomacs-webkit-test.el ends here
