(progn
  (defun gdj-source-storage (s)
    (list (length s) (string-bytes s) (multibyte-string-p s)
          (append (string-as-unibyte s) nil)))
  (defun gdj-source-object (value)
    (if (symbolp value)
        (list 'symbol (gdj-source-storage (symbol-name value)))
      (list 'object value)))
  (defun gdj-source-read (s mode start end)
    (condition-case caught
        (if (eq mode 'from-string)
            (let ((pair (cond
                          ((and (null start) (null end)) (read-from-string s))
                          ((null end) (read-from-string s start))
                          (t (read-from-string s start end)))))
              (list 'ok (gdj-source-object (car pair)) (cdr pair)))
          (list 'ok (gdj-source-object
                     (if (eq mode 'standard-input)
                         (let ((standard-input s)) (read))
                       (read s)))))
      ;; Keep only the condition symbol: do not print an array-bearing error.
      (error (list 'error (car caught)))))
  (defun gdj-source-cases (tag source properties)
    (let ((read-symbol-shorthands nil))
      (when properties (setq source (propertize source 'face 'bold)))
      (list
       tag properties (gdj-source-storage source)
       (list 'read (gdj-source-read source 'read nil nil))
       (list 'standard-input (gdj-source-read source 'standard-input nil nil))
       (list 'default (gdj-source-read source 'from-string nil nil))
       (list 'start-zero (gdj-source-read source 'from-string 0 nil))
       (list 'full-bound (gdj-source-read source 'from-string 0 (length source)))
       (list 'empty-bound (gdj-source-read source 'from-string 0 0))
       (list 'at-end (gdj-source-read source 'from-string (length source) nil))
       (list 'empty-at-end
             (gdj-source-read source 'from-string (length source) (length source)))
       (list 'negative-start (gdj-source-read source 'from-string -1 nil))
       (list 'one-character-bound (gdj-source-read source 'from-string 0 1))
       (list 'two-character-bound (gdj-source-read source 'from-string 0 2))
       (list 'negative-end (gdj-source-read source 'from-string 0 -1)))))
  (let* ((short (unibyte-string #xc3 #xa9))
         (query (string-to-multibyte "é"))
         (table (obarray-make))
         (declared-one
          (let ((read-symbol-shorthands
                 (list (cons short (string-to-multibyte "bé")))))
            (symbol-name (intern query table))))
         (declared-zero
          (let ((read-symbol-shorthands (list (cons short "a"))))
            (symbol-name (intern query (obarray-make)))))
         ;; Reading declared-one consumes only initialized ASCII b, then
         ;; SCHARS=1 ends the stream before initialized C3 A9. Zero reads none.
         (sources (list (cons 'declared-one declared-one)
                        (cons 'declared-zero declared-zero)
                        (cons 'ordinary-multibyte (string-to-multibyte "bé"))
                        (cons 'ordinary-unibyte (unibyte-string ?b #xc3 #xa9))
                        (cons 'ascii-multibyte (string-to-multibyte "b"))))
         results)
    (dolist (entry sources)
      (setq results
            (cons (gdj-source-cases (car entry) (cdr entry) nil) results))
      (setq results
            (cons (gdj-source-cases (car entry) (cdr entry) t) results)))
    (nreverse results)))
