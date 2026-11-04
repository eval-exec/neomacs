(progn
  (require 'bytecomp)
  (defvar gdj-nine-compiled nil)
  (defvar gdj-nine-input nil)
  (defvar gdj-nine-gets 0)
  (defvar gdj-nine-ungets 0)
  (defun gdj-nine-get (&optional character)
    (if character
        (progn (setq gdj-nine-ungets (1+ gdj-nine-ungets)
                     gdj-nine-input (cons character gdj-nine-input)) nil)
      (setq gdj-nine-gets (1+ gdj-nine-gets))
      (let ((character (car gdj-nine-input)))
        (setq gdj-nine-input (cdr gdj-nine-input)) character)))
  (defun gdj-nine-metadata (name)
    ;; Only convert fully initialized storage to unibyte before iteration.
    (list (length name) (string-bytes name) (multibyte-string-p name)
          (append (string-as-unibyte name) nil)))
  (defun gdj-nine-stream (source kind)
    (cond
     ((eq kind 'string)
      (let ((pair (read-from-string source)))
        (list (car pair) (cdr pair))))
     ((eq kind 'buffer)
      (with-temp-buffer
        (set-buffer-multibyte t) (insert source) (goto-char 1)
        (let ((result (read (current-buffer)))) (list result (point)))))
     ((eq kind 'marker)
      (with-temp-buffer
        (set-buffer-multibyte t) (insert source) (goto-char 1)
        (let* ((marker (copy-marker 1)) (result (read marker)))
          (list result (list (marker-position marker) (point))))))
     (t
      (let* ((gdj-nine-input (append source nil))
             (gdj-nine-gets 0) (gdj-nine-ungets 0)
             (function
              (cond ((eq kind 'named) 'gdj-nine-get)
                    ((eq kind 'lambda)
                     (lambda (&optional c) (gdj-nine-get c)))
                    (t gdj-nine-compiled)))
             (result (read function)))
        (list result (list gdj-nine-input gdj-nine-gets gdj-nine-ungets))))))
  (defun gdj-nine-case (branch kind properties)
    (let* ((prefix (format "gdj-nine-global-only-%s-%s-%s-"
                           branch kind properties))
           (short (if (eq branch 'normal-control)
                      (string-to-multibyte "é")
                    (unibyte-string #xc3 #xa9)))
           (long (concat prefix
                         (if (eq branch 'mixed-ascii-long) "b"
                           (string-to-multibyte "bé"))))
           (query (string-to-multibyte "éé"))
           (source (concat query " ")))
      (when properties
        (setq short (propertize short 'face 'bold)
              long (propertize long 'help-echo "long")
              source (propertize source 'face 'italic)))
      (let* ((read-symbol-shorthands (list (cons short long)))
             (pair (gdj-nine-stream source kind))
             (symbol (car pair)) (name (symbol-name symbol))
             (raw (string-as-unibyte name))
             (decoded (decode-coding-string raw 'utf-8)))
        (list branch kind properties
              (gdj-nine-metadata name) (cadr pair)
              (eq (intern-soft query) symbol)
              (eq (intern-soft symbol) symbol)
              (let ((read-symbol-shorthands nil))
                (list (eq (intern-soft name) symbol)
                      (eq (intern-soft raw) symbol)
                      (eq (intern-soft decoded) symbol)))))))
  ;; Compile once; all reads/lookups use the ordinary global obarray.
  (setq gdj-nine-compiled
        (byte-compile '(lambda (&optional c) (gdj-nine-get c))))
  (let (results)
    (dolist (branch '(mixed-ascii-long mixed-multibyte-long normal-control))
      (dolist (kind '(string buffer marker named lambda compiled))
        (dolist (properties '(nil t))
          (setq results (cons (gdj-nine-case branch kind properties) results)))))
    (nreverse results)))
