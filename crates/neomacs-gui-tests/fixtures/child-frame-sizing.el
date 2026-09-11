;;; child-frame-sizing.el --- Fit before first redisplay -*- lexical-binding: t -*-

(run-at-time
 1 nil
 (lambda ()
   (let* ((frame-resize-pixelwise t)
          (buffer (get-buffer-create "*child-sizing*"))
          (parent (selected-frame)))
     (with-current-buffer buffer
       (setq-local mode-line-format nil)
       (setq-local header-line-format nil)
       (setq-local tab-line-format nil)
       (insert "TestABC"))
     (let ((child (make-frame `((parent-frame . ,parent)
                                (minibuffer . nil) (visibility . nil)
                                (width . 20) (height . 1)
                                (min-width . 0) (min-height . 0)
                                (menu-bar-lines . 0) (tool-bar-lines . 0)
                                (tab-bar-lines . 0) (internal-border-width . 3)))))
       (set-window-buffer (frame-root-window child) buffer)
       ;; Same fitting entry point as posframe, before any child redisplay.
       (fit-frame-to-buffer-1 child 40 1 50 1 nil nil nil)
       (make-frame-visible child)
       (run-at-time
        1 nil
        (lambda ()
          (neomacs--write-frame-snapshot
           (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json)
          (run-at-time 1 nil (lambda () (kill-emacs 0)))))))))
