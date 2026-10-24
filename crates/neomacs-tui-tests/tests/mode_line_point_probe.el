;;; -*- lexical-binding: t; -*-
;;; Native percent decoding must observe preceding :eval effects in order.

(let ((out (getenv "D16_POINT_OUT")))
  (when noninteractive
    (error "Point oracle requires an initialized interactive TTY"))
  (unless out (error "Point oracle requires an owned output path"))
  (switch-to-buffer (get-buffer-create "point-percent-oracle"))
  (fundamental-mode)
  (setq mode-line-format nil header-line-format nil tab-line-format nil)
  (erase-buffer)
  (insert "alpha\nbravo words\ncharlie\ndelta\necho\n")
  (goto-char (point-min))
  ;; All three specs repeat inside each expansion to exercise their shared
  ;; native calculation, while :eval boundaries must discard that state.
  (let ((value
         (format-mode-line
          '("start[%l/%c/%C]"
            (:eval (progn (forward-line 2) (forward-char 2) ""))
            "forward[%l/%c/%C][%l/%c/%C]"
            (:eval (progn (forward-line -1) (forward-char 4) ""))
            "backward[%l/%c/%C]"
            (:eval (progn
                     (goto-char (point-min))
                     (forward-line 1)
                     (narrow-to-region (point) (point-max))
                     (forward-line 1)
                     (forward-char 2)
                     ""))
            "narrow[%l/%c/%C]"
            (:eval (progn (widen) ""))
            "widen[%l/%c/%C]"
            (:eval (save-excursion (goto-char (point-max)) ""))
            "excursion[%l/%c/%C]"
            (:eval (progn (goto-char (point-min)) ""))
            "restored[%l/%c/%C]"))))
    (with-temp-file out
      (prin1 (substring-no-properties value) (current-buffer))
      (insert "\n"))))
(kill-emacs 0)
