;;; display-line-numbers.el --- Display replacement gutter regression -*- lexical-binding: t; -*-
(switch-to-buffer (get-buffer-create "*display-gutter*"))
(setq-local mode-line-format nil header-line-format nil cursor-type nil)
(dotimes (i 5) (insert (format "line %d\n" (1+ i))))
(display-line-numbers-mode 1)
(set-face-attribute 'line-number nil :background "#101010")
(set-face-attribute 'line-number-current-line nil :background "#ffff00")
(goto-char (point-min))
(if (equal (getenv "NEOMACS_LINE_NUMBER_CASE") "hidden")
    (progn
      (forward-line 1)
      (put-text-property (point) (progn (forward-line 2) (point)) 'display "[hidden] ")
      (goto-char (point-max))
      (forward-line -1))
  (let ((eol (line-end-position)))
    (overlay-put (make-overlay eol (1+ eol)) 'display "  [A]\n  [B]\n"))
  (forward-line 2))
;; A blue text background independently identifies the actual point line in
;; the GPU readback. Its yellow gutter must occupy the same scanlines.
(put-text-property (line-beginning-position) (line-end-position)
                   'face '(:background "#0000ff"))
(redisplay t)
(run-at-time 2 nil (lambda () (kill-emacs 0)))
