;;; n832-category-cursor-oracle.el --- Category face put, then a cursor move -*- lexical-binding: t -*-
;;;
;;; Point sits on a row whose text has a `category' property. The category
;;; symbol's `face' changes from height 100 to 200, then point moves inside
;;; the same window. `window-line-height' reads the current matrix
;;; (window.c:2082-2089) and does not rebuild glyphs.
;;; `Fput' sets no redisplay flag (fns.c:2729-2737). `try_cursor_movement'
;;; keeps the old row (xdisp.c:19867-19902).
(setq inhibit-startup-screen t)
(select-frame (make-frame '((width . 80) (height . 24))))
(switch-to-buffer (get-buffer-create "*n832-cat*"))
(erase-buffer)
(insert "AAAA\nBBBB\nCCCC\n")
(put 'n832-cat 'face '(:height 100))
(put-text-property 1 5 'category 'n832-cat)
(goto-char 1)
(redisplay)
(let ((attach (window-line-height 0)))
  (put 'n832-cat 'face '(:height 200))
  (redisplay)
  (let ((after-put (window-line-height 0)))
    (forward-char 1)
    (redisplay)
    (let ((after-move (window-line-height 0)))
      (force-window-update (selected-window))
      (redisplay)
      (princ (format "CATEGORY-ROW attach=%S put=%S move=%S force=%S point=%s\n"
                     attach after-put after-move (window-line-height 0) (point))
             'external-debugging-output))))
(kill-emacs 0)
