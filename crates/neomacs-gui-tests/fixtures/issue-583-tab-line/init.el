;;; issue-583-tab-line/init.el --- Startup tab-line regression -*- lexical-binding: t; -*-

;; Exercise the public init path, including buffers created before and after
;; global mode activation.  Do not explicitly turn on the buffer-local mode.
(setq inhibit-startup-screen t
      initial-scratch-message "Issue 583 startup scratch buffer\n")
(global-tab-line-mode 1)

(defun issue-583-tab-line-snapshot (tag)
  (neomacs--write-frame-snapshot
   (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") "." tag)
   (selected-frame) 'json)
  (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
             (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") "." tag ".png") t))

(defun issue-583-tab-line-finish ()
  (issue-583-tab-line-snapshot "flat-button")
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON") (insert "{}\n"))
  (kill-emacs 0))

(defun issue-583-tab-line-box ()
  (issue-583-tab-line-snapshot "split")
  ;; GNU accepts an omitted box color and resolves it from the face.
  (set-face-attribute 'tab-line-tab-current nil
                      :foreground "#ff0000" :background "#00ff00"
                      :box '(:line-width 2 :style flat-button))
  (force-mode-line-update t)
  (run-with-timer 1 nil #'issue-583-tab-line-finish))

(defun issue-583-tab-line-split ()
  (issue-583-tab-line-snapshot "visited")
  (split-window-right)
  (set-window-buffer (next-window) (get-buffer "*scratch*"))
  (run-with-timer 1 nil #'issue-583-tab-line-box))

(defun issue-583-tab-line-visit ()
  (issue-583-tab-line-snapshot "scratch")
  (find-file (expand-file-name "issue-583-visited.txt"
                               (file-name-directory
                                (getenv "NEOMACS_GUI_STATE_JSON"))))
  (insert "Issue 583 newly visited file\n")
  (set-buffer-modified-p nil)
  (run-with-timer 1 nil #'issue-583-tab-line-split))

(run-with-timer 2 nil #'issue-583-tab-line-visit)
(run-with-timer 20 nil (lambda () (message "Issue 583 watchdog expired") (kill-emacs 2)))
