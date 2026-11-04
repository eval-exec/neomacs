(let ((make-source
       (lambda (count)
         (let* ((query (make-string (if (= count 0) 5 3) 233))
                (short (encode-coding-string query 'utf-8 t))
                (read-symbol-shorthands
                 (list (cons short (string-as-multibyte "abcde")))))
           (symbol-name (intern query (obarray-make)))))))
  (apply 'append
   (mapcar (lambda (count)
    (apply 'append
     (mapcar (lambda (properties)
      (apply 'append
       (mapcar (lambda (start)
        (mapcar (lambda (pattern)
         (let* ((source (funcall make-source count))
                (parse-sexp-lookup-properties t)
                (read-symbol-shorthands nil)
                (case-fold-search nil))
           (when properties (put-text-property 0 count 'face 'bold source))
           (set-match-data '(11 12))
           (let ((result
                  (condition-case err
                      (list 'ok (string-match pattern source start) (match-data))
                    (error (list 'signal err (match-data))))))
             (list count properties start pattern
                   (length source) (length (string-as-unibyte source))
                   (append (string-as-unibyte source) nil) result))))
         '("\\b" "\\B" "\\<" "\\>" "\\_<" "\\_>" "a\\b" "b\\b" "c\\b")))
        (if (= count 0) '(nil 0) '(nil 0 1 2)))))
      '(nil t))))
    '(0 2))))
