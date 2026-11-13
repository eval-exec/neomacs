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
                      (lambda (operation)
                        (let* ((source (funcall make-source count))
                               (read-symbol-shorthands nil)
                               (case-fold-search nil))
                          (set-match-data nil)
                          (let* ((result (string-match pattern source nil))
                                 (start (match-beginning 0))
                                 (end (match-end 0))
                                 (published (match-data)))
                            (list operation result published
                                  (if (and (eq operation 'replace-match)
                                           start end (> start end))
                                      '(excluded-gnu-ordered-register-assertion)
                                    (condition-case err
                                        (list 'ok
                                              (funcall describe
                                                       (if (eq operation 'match-string)
                                                           (match-string 0 source)
                                                         (replace-match "X" t t source 0))))
                                      (error
                                       (cons (car err) (mapcar describe (cdr err))))))))))
                      '(match-string replace-match))))
             '("d" "bcd" "a\\(bcd\\)" "bc" "abcd" "ab" "abc" "a\\(b\\)"))))
    '(0 2))
   (mapcar
    (lambda (count)
      (mapcar
       (lambda (handlers)
         (let ((source (funcall make-source count))
               (read-symbol-shorthands nil)
               (file-name-handler-alist handlers))
           (set-match-data '(11 12))
           (list count handlers
                 (find-file-name-handler source 'gdj-handler-operation)
                 (match-data))))
       '((("d" . gdj-rightmost))
         (("b" . gdj-first) ("d" . gdj-rightmost)))))
    '(0 2))
   (mapcar
    (lambda (count)
      (let ((source (funcall make-source count)))
        (list count
              (funcall describe (concat source "X"))
              (funcall describe (concat source (vector ?X))))))
    '(0 2))
   (mapcar
    (lambda (pattern)
      (mapcar
       (lambda (index)
         (let ((source (funcall make-source 2))
               (read-symbol-shorthands nil) (case-fold-search nil))
           (set-match-data nil)
           (let ((matched (and pattern (string-match pattern source nil))))
             (list pattern index matched (match-data)
                   (condition-case err
                       (list 'ok (aset source index ?X) (funcall describe source))
                     (error (cons (car err) (mapcar describe (cdr err)))))))))
       '(0 1)))
    '(nil "d" "abcd"))
   (mapcar
    (lambda (pattern)
      (mapcar
       (lambda (from)
         (mapcar
          (lambda (to)
            (let ((source (funcall make-source 2))
                  (read-symbol-shorthands nil) (case-fold-search nil))
              (set-match-data nil)
              (let ((matched (and pattern (string-match pattern source nil))))
                (list pattern from to matched (match-data)
                      (condition-case err
                          (list 'ok (string-width source from to))
                        (error err))))))
          '(nil 2)))
       '(0 1)))
    '(nil "d" "abcd"))))
