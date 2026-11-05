(let ((make-source
       (lambda (count)
         (let* ((query (make-string (- 5 count) 233))
                (short (encode-coding-string query 'utf-8 t))
                (read-symbol-shorthands
                 (list (cons short (string-as-multibyte "abcde")))))
           (symbol-name (intern query (obarray-make))))))
      (describe
       (lambda (value)
         (if (stringp value)
             (list 'string (length value) (length (string-as-unibyte value))
                   (multibyte-string-p value)
                   (append (string-as-unibyte value) nil))
           value))))
  (list
   (mapcar
    (lambda (count)
      (list count
            (mapcar
             (lambda (pattern)
               (list pattern
                     (mapcar
                      (lambda (replacement)
                        (list replacement
                              (mapcar
                               (lambda (fixedcase)
                                 (mapcar
                                  (lambda (literal)
                                    (let ((source (funcall make-source count))
                                          (read-symbol-shorthands nil)
                                          (case-fold-search nil)
                                          (text-quoting-style 'straight))
                                      (set-match-data nil)
                                      (let* ((matched (string-match pattern source nil))
                                             (start (match-beginning 0))
                                             (end (match-end 0))
                                             (data (match-data)))
                                        (list fixedcase literal matched data
                                              (if (and start end (> start end))
                                                  '(excluded-gnu-ordered-register-assertion)
                                                (condition-case err
                                                    (list 'ok
                                                          (funcall describe
                                                                   (replace-match replacement fixedcase literal source 0)))
                                                  (error
                                                   (cons (car err) (mapcar describe (cdr err))))))))))
                                  '(nil t)))
                               '(nil t))))
                      '("X" "x\\&y" "\\1" "\\?" "\\" "\\n"))))
             '("a" "ab" "a\\(b\\)" "d"))))
    '(0 2))
   ;; Ordered whole-match registers (0,0) permit replace-match. The signed
   ;; subgroup END=-1 is a valid relative substring endpoint in this target.
   (mapcar
    (lambda (replacement)
      (mapcar
       (lambda (fixedcase)
         (mapcar
          (lambda (literal)
            (let ((source (funcall make-source 0))
                  (target "uvwxy")
                  (read-symbol-shorthands nil) (case-fold-search nil)
                  (text-quoting-style 'straight))
              (set-match-data nil)
              (let ((matched (string-match "ab\\(cd\\)e" source nil))
                    (data (match-data)))
                (list replacement fixedcase literal matched data
                      (condition-case err
                          (list 'ok (funcall describe
                                             (replace-match replacement fixedcase literal target 0)))
                        (error (cons (car err) (mapcar describe (cdr err)))))))))
          '(nil t)))
       '(nil t)))
    '("x\\&y" "\\1"))
   (mapcar
    (lambda (group)
      (let ((source (funcall make-source 0)) (target "uvwxy")
            (read-symbol-shorthands nil) (case-fold-search nil))
        (set-match-data nil)
        (let ((matched (string-match "ab\\(cd\\)e" source nil))
              (data (match-data)))
          (list group matched data
                (condition-case err
                    (list 'ok (funcall describe (match-string group target)))
                  (error (cons (car err) (mapcar describe (cdr err)))))))))
    '(0 1))))
