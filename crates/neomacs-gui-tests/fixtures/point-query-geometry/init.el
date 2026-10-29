;;; Point-dependent geometry must be queried before redisplay. -*- lexical-binding: t; -*-
(require 'json)

(defun point-query-geometry-probe ()
  (switch-to-buffer (get-buffer-create "*point-query-geometry*"))
  (erase-buffer)
  (dotimes (_ 200) (insert "a\n"))
  (let ((out nil))
    (push (cons "native-engine"
                (if (fboundp 'neomacs--write-frame-snapshot) t :json-false)) out)
    (dolist (variable '(header-line-format tab-line-format mode-line-format))
      (setq-local header-line-format nil tab-line-format nil mode-line-format nil)
      (setq-local display-line-numbers nil)
      (goto-char 1)
      (set-window-start (selected-window) 1)
      (set (make-local-variable variable)
           '((:eval (propertize "H" 'face
                     (if (= (point) 1) '(:height 1.0) '(:height 4.0))))))
      (redisplay t)
      (let ((target 1))
        (while (and (< (+ target 2) (point-max))
                    (pos-visible-in-window-p (+ target 2) nil nil))
          (setq target (+ target 2)))
        (let ((before (posn-at-point target)))
          (goto-char 3)
          ;; Deliberately query before redisplay: point changes the header's
          ;; height without changing the text or the chrome format's identity.
          (push (cons (symbol-name variable)
                      (vector (if before t :json-false)
                              (if (posn-at-point target) t :json-false))) out))))
    (setq-local header-line-format nil tab-line-format nil mode-line-format nil)
    (setq-local display-line-numbers t)
    (set-face-attribute 'line-number-current-line nil :inherit 'line-number :height 2.0)
    (goto-char 1)
    (set-window-start (selected-window) 1)
    (redisplay t)
    (let ((before (posn-x-y (posn-at-point 3))))
      (goto-char 3)
      (let ((after (posn-x-y (posn-at-point 3))))
        (push (cons "current-line-face"
                    (vector (car before) (cdr before) (car after) (cdr after))) out)))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode (nreverse out)))))
  (when (fboundp 'neomacs--write-frame-snapshot)
    (neomacs--write-frame-snapshot
     (concat (getenv "NEOMACS_GUI_STATE_JSON") ".frame.json") nil 'json))
  (kill-emacs 0))

(run-with-timer 1 nil
  (lambda ()
    (condition-case err (point-query-geometry-probe)
      (error
       (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
         (insert (json-encode (list (cons "error" (format "%S" err))))))
       (kill-emacs 1)))))
