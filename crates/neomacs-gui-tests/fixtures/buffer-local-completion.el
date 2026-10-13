;;; Buffer-local completion in a live minibuffer. -*- lexical-binding: t; -*-
(require 'timer)
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(run-at-time
 0.5 nil
 (lambda ()
   (unless (display-graphic-p) (error "Expected a graphical frame"))
   (condition-case err
       (let* ((completion-ignore-case nil)
              (selected
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq-local completion-ignore-case nil)
                     (let ((completion-ignore-case t))
                       (insert "*mes")
                       (minibuffer-complete))
                     (run-at-time 0.3 nil #'exit-minibuffer))
                 (completing-read "Case: " '("*Messages*") nil t)))
              (passed (equal selected "*Messages*")))
         (switch-to-buffer (get-buffer-create "*completion result*"))
         (erase-buffer)
         (insert (format "Selected: %s\n" selected))
         (message nil)
         (redisplay t)
         (let ((snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT")))
           (when (and snapshot (fboundp 'neomacs--write-frame-snapshot))
             (neomacs--write-frame-snapshot snapshot t 'text-faces)))
         (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
           (insert (format "{\"passed\":%s,\"selected\":\"%s\"}\n"
                           (if passed "true" "false") selected)))
         (run-at-time 0.5 nil (lambda () (kill-emacs (if passed 0 1)))))
     (error (message "Completion fixture: %S" err) (kill-emacs 1)))))
(run-at-time 8 nil (lambda () (kill-emacs 2)))
