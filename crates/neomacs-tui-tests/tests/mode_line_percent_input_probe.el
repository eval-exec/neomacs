;;; -*- lexical-binding: t; -*-
;;; Exercise native percent inputs across live format-mode-line evaluations.

(defvar d15-pct-events nil)

(defun d15-pct-state (label)
  (list label (buffer-name) buffer-file-name buffer-read-only
        (buffer-modified-p) (buffer-narrowed-p)
        (point) (point-min) (point-max)))

(defun d15-pct-note (label)
  (push (d15-pct-state label) d15-pct-events)
  "")

(defun d15-pct-observe (string)
  ;; Inspect only the fixture's public property, not GNU's extra annotations
  ;; or the implementation's interval/plist representation.
  (list (substring-no-properties string)
        (multibyte-string-p string)
        (length string)
        (string-width string)
        (let ((i 0) (properties nil))
          (while (< i (length string))
            (push (get-text-property i 'd15-pct-property string) properties)
            (setq i (1+ i)))
          (nreverse properties))))

(let ((out (getenv "D15_PERCENT_OUT"))
      (ready (getenv "D15_PERCENT_READY"))
      (release (getenv "D15_PERCENT_RELEASE"))
      (observations nil))
  (when noninteractive
    (error "D15 requires an initialized interactive TTY, not batch formatting"))
  (unless (and out ready release)
    (error "D15 requires owned OUT/READY/RELEASE paths"))
  (switch-to-buffer (get-buffer-create "d15-pct-before"))
  (fundamental-mode)
  (setq mode-line-format nil header-line-format nil tab-line-format nil
        buffer-read-only nil)
  (erase-buffer)
  (insert "alpha\nbravo words\ncharlie\ndelta\n")
  (goto-char (point-min))
  (setq buffer-file-name "/tmp/d15-pct-before.txt")
  (set-buffer-modified-p nil)
  (setq d15-pct-events nil)
  ;; Repeated b/f in EACH string exercise one native expansion. Mutations in
  ;; :eval separate expansions and must remain observable to later strings.
  ;; Set the native file-name slot directly: GNU has no set-buffer-file-name.
  (let ((formatted
         (format-mode-line
          '("before[%b|%b|%f|%f]"
            (:eval (progn
                     (rename-buffer "d15-pct-after")
                     (setq buffer-file-name "/tmp/d15-pct-after.txt")
                     (d15-pct-note 'rename-file)))
            "after[%b|%b|%f|%f]"
            (:eval (progn (setq buffer-file-name nil)
                          (d15-pct-note 'nil-file)))
            "nil[%f|%f|%b|%b]"
            (:eval (progn (setq buffer-file-name "")
                          (d15-pct-note 'empty-file)))
            "empty[%f|%f|%b|%b]"
            (:eval (progn
                     (rename-buffer "d15-café")
                     (setq buffer-file-name "/tmp/d15-é.txt")
                     (d15-pct-note 'multibyte-name)))
            (:propertize "·%16b/%24f!" d15-pct-property width)))))
    (push (list 'names (d15-pct-observe formatted)
                (nreverse d15-pct-events) (d15-pct-state 'end-names))
          observations))
  (setq buffer-read-only nil)
  (set-buffer-modified-p nil)
  (widen)
  (goto-char (point-min))
  (setq d15-pct-events nil)
  (let ((formatted
         (format-mode-line
          '("clean[%*|%+|%&|%n|%l|%c|%C][%*|%+|%&]"
            (:eval (progn (setq buffer-read-only t)
                          (set-buffer-modified-p t)
                          (d15-pct-note 'readonly-modified)))
            "both[%*|%+|%&|%n|%l|%c|%C][%*|%+|%&]"
            (:eval (progn (set-buffer-modified-p nil)
                          (d15-pct-note 'readonly-clean)))
            "readonly[%*|%+|%&]"
            (:eval (progn
                     (setq buffer-read-only nil)
                     (set-buffer-modified-p t)
                     (goto-char (point-min))
                     (forward-line 1)
                     (narrow-to-region (point) (point-max))
                     (forward-line 1)
                     (forward-char 2)
                     (d15-pct-note 'narrow-point)))
            "narrow[%*|%+|%&|%n][%n]"
            (:eval (progn (widen) (goto-char (point-min))
                          (set-buffer-modified-p nil)
                          (d15-pct-note 'restore)))
            "restored[%*|%+|%&|%n|%l|%c|%C]"))))
    (push (list 'flags-point (d15-pct-observe formatted)
                (nreverse d15-pct-events) (d15-pct-state 'end-flags))
          observations))
  ;; No side-effecting :eval remains during actual redisplay/READY waiting.
  ;; This additionally sends the same decoder family through DISPLAY mode.
  (setq mode-line-format
        '("D15[" "%b|%b|%f|%f|%*|%+|%&|%n|%l|%c" "]"))
  (force-mode-line-update)
  (redisplay t)
  (with-temp-file out
    (let ((print-length nil) (print-level nil) (print-escape-nonascii t))
      (prin1 (list 'd15-percent-transition-v1 (nreverse observations))
             (current-buffer))
      (insert "\n")))
  (let ((inhibit-redisplay t))
    (with-temp-file ready (insert "ready\n"))
    (while (not (file-exists-p release))
      (accept-process-output nil 0.01))))
(kill-emacs 0)
