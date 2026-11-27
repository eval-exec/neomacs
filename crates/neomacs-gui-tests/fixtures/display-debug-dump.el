;;; display-debug-dump.el --- Live Wayland display debug dump contract -*- lexical-binding: t; -*-

(run-at-time
 0.5 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (display-graphic-p)
           (error "Expected a graphical frame"))
         (unless (fboundp 'neomacs-dump-display-state)
           (error "neomacs-dump-display-state is not defined"))
         (unless (eq (lookup-key menu-bar-help-menu [neomacs-dump-display-state])
                     'neomacs-dump-display-state)
           (error "Help -> Dump Display Debug State menu item missing: %S"
                  (lookup-key menu-bar-help-menu [neomacs-dump-display-state])))
         (let ((buf (get-buffer-create "*display-debug-test*")))
           (switch-to-buffer buf)
           (erase-buffer)
           (insert "line 0 alpha\n"
                   "line 1 beta\n"
                   "line 2 gamma cursor-here\n"
                   "line 3 delta\n"
                   "line 4 epsilon\n")
           (goto-char (point-min))
           (forward-line 2)
           (forward-char 4)
           ;; Allow a full redisplay + render-thread swapchain present.
           (redisplay t)
           (sit-for 0.3)
           ;; Exercise the exact interactive/menu path with $NEOMACS_DEBUG_DISPLAY_DUMP
           ;; and verify it does not activate the minibuffer.
           (let ((written (call-interactively #'neomacs-dump-display-state)))
             (when (active-minibuffer-window)
               (error "neomacs-dump-display-state unexpectedly opened minibuffer"))
             (unless (and (stringp written) (file-exists-p written))
               (error "neomacs-dump-display-state did not create file: %S" written))))
         (princ "DISPLAY-DEBUG-PASS\n" 'external-debugging-output)
         (kill-emacs 0))
     (error
      (princ (format "DISPLAY-DEBUG-FAIL %S\n" err) 'external-debugging-output)
      (kill-emacs 1)))))
