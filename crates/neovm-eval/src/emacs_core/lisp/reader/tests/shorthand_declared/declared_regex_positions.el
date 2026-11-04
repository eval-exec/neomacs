(let ((make-source
       (lambda ()
         (let* ((query (make-string 5 233))
                (short (encode-coding-string query 'utf-8 t))
                (long (string-as-multibyte "abcde"))
                (read-symbol-shorthands (list (cons short long))))
           (symbol-name (intern query (obarray-make)))))))
  (mapcar
   (lambda (pattern)
     (list pattern
           (mapcar
            (lambda (start)
              (let* ((source (funcall make-source))
                     (read-symbol-shorthands nil)
                     (case-fold-search nil)
                     (bytes (string-as-unibyte source)))
                (set-match-data nil)
                (list start (length source) (length bytes)
                      (multibyte-string-p source) (append bytes nil)
                      (condition-case err
                          (let ((result (string-match pattern source start)))
                            (if result
                                (list 'ok result (match-data)
                                      (match-beginning 0) (match-end 0)
                                      (match-beginning 1) (match-end 1)
                                      (match-beginning 2) (match-end 2))
                              '(miss)))
                        (error err)))))
            '(nil 0))))
   '("d" "^" "$" "de" "bcd" "a\\(bcd\\)" "\\(d\\)" "\\(b\\)cd\\(e\\)")))
