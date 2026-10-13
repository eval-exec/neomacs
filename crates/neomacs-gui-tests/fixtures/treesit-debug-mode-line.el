;;; treesit-debug-mode-line.el --- Issue #543 -*- lexical-binding: t -*-

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (require 'treesit)
         (setq user-emacs-directory (getenv "NEOMACS_TEST_GRAMMAR_USER_DIR")
               treesit-extra-load-path nil)
         (switch-to-buffer (get-buffer-create "issue543-go"))
         (erase-buffer)
         (insert "package main\nfunc main() {}\n")
         (let ((root (treesit-parser-root-node (treesit-parser-create 'go))))
           (unless (and (equal (treesit-node-type root) "source_file")
                        (not (treesit-node-check root 'has-error)))
             (error "Go grammar did not parse")))
         (goto-char (point-max))
         (insert "// Parsed go: source_file\n")
         (goto-char (point-min))
         (when (display-graphic-p) (set-frame-font "DejaVu Sans Mono-10" nil t))
         (set-frame-size nil 180 30)
         (set-face-attribute 'mode-line nil :foreground "black" :background "white" :box nil)
         (setq-local mode-line-format (list (concat (make-string 118 ?a) "…END")))
         (redisplay t)
         (with-temp-file (getenv "NEOMACS_TEST_TREESIT_READY") (insert "ready"))
         (when (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
           (run-at-time
            1 nil
            (lambda ()
              (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                         (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
              (kill-emacs 0)))))
     (error (message "Issue 543: %S" err) (kill-emacs 1)))))
(run-at-time 20 nil (lambda () (message "Issue 543 watchdog expired") (kill-emacs 2)))
