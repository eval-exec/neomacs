;;; window-chrome.el --- Native chrome theme/multi-frame fixture -*- lexical-binding: t -*-

(switch-to-buffer (get-buffer-create "*native-window-chrome*"))
(erase-buffer)
(insert "Native titlebar background verification\n\n"
        "The primary frame changes from magenta to green.\n"
        "The secondary frame stays blue.\n"
        "Native controls must remain visible and clickable.\n"
        "Check Help/menu hover, text selection, titlebar drag and fullscreen.\n")
(goto-char (point-min))
(set-background-color "#ff00ff")
(set-foreground-color "#000000")
(defvar neomacs-chrome-test-primary (selected-frame))
(defvar neomacs-chrome-test-secondary
  (make-frame '((name . "Native chrome secondary")
                (background-color . "#0000ff")
                (foreground-color . "#ffffff"))))

(run-at-time 1 nil
             (lambda ()
               (modify-frame-parameters neomacs-chrome-test-primary
                                        '((background-color . "#00ff00")))))

;; Without the harness environment this remains open for manual macOS checks.
(when (getenv "NEOMACS_GUI_STATE_JSON")
  (run-at-time 3 nil
               (lambda ()
                 (unless (and (frame-live-p neomacs-chrome-test-primary)
                              (frame-live-p neomacs-chrome-test-secondary))
                   (error "Native chrome fixture lost a frame"))
                 (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
                   (insert "{\"primary_alive\":true,\"secondary_alive\":true}\n"))
                 (kill-emacs 0))))
