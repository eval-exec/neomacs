(progn
  (defun gdj-twelve-source (prefix terminal properties)
    (let* ((read-symbol-shorthands
            (list (cons (string #xe9) (string-as-multibyte prefix))))
           (query (if terminal (unibyte-string #xc3 #xa9) (string #xe9)))
           (source (symbol-name (intern query (obarray-make)))))
      (if properties (propertize source 'gdj-unread-property 17) source)))
  (defun gdj-twelve-metadata (source)
    (list (length source) (string-bytes source) (multibyte-string-p source)
          (append (string-as-unibyte source) nil)))
  (defun gdj-twelve-read (source start end)
    (let ((read-symbol-shorthands nil))
      (condition-case caught
          (let ((pair (read-from-string source start end)))
            (list 'ok
                  (if (symbolp (car pair))
                      (list 'symbol (symbol-name (car pair)))
                    (car pair))
                  (cdr pair)))
        (error (list 'error (car caught) (cdr caught))))))
  (defun gdj-twelve-case (prefix terminal properties)
    (let ((source (gdj-twelve-source prefix terminal properties))
          (fresh (gdj-twelve-source prefix terminal properties)))
      (list prefix terminal properties (gdj-twelve-metadata source)
            (list 'fresh-start-one (gdj-twelve-read fresh 1 2))
            (list 'first-full-read (gdj-twelve-read source 0 (length source)))
            (list 'same-after-read-start-one (gdj-twelve-read source 1 2))
            (list 'fresh-after-read-start-one
                  (gdj-twelve-read
                   (gdj-twelve-source prefix terminal properties) 1 2))
            (list 'same-reset-start-zero (gdj-twelve-read source 0 1))
            (list 'same-after-reset-start-one (gdj-twelve-read source 1 2)))))
  (let (results)
    ;; Every source has at least two initialized ASCII bytes. The only extra
    ;; declared character (when terminal=t) is GNU's one initialized NUL.
    ;; Numeric bounds avoid the separate inherited explicit-nil-END issue.
    (dolist (prefix '("#^" "#^^" "#s" "#1" "#x1" "#o7" ".a"))
      (dolist (terminal '(nil t))
        (dolist (properties '(nil t))
          (setq results
                (cons (gdj-twelve-case prefix terminal properties) results)))))
    (nreverse results)))
