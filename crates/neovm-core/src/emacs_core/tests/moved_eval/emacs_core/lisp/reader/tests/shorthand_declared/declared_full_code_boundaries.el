(let ((make-source
       (lambda (code mode)
         ;; Initialized payload has five physical characters. Shorthand
         ;; supplies 5(long)-1(short)+2(unibyte query)=6 logical characters.
         ;; Two byte8 characters ensure at least seven bytes, so this enters
         ;; nonidentity supplied multibyte syntax state, never SCHARS==SBYTES.
         ;; The one extra logical character is the initialized terminator;
         ;; do not append/aref or otherwise iterate the source's characters.
         (let* ((query (encode-coding-string (string #xe9) 'utf-8 t))
                (short (string #xe9))
                (long (concat "a" (string code) "b" (string code) "c"))
                (read-symbol-shorthands (list (cons short long)))
                (name (symbol-name (intern query (obarray-make)))))
           (cond ((eq mode 'supplied-multibyte) name)
                 ((eq mode 'computed-multibyte) (copy-sequence long))
                 (t (string-as-unibyte name))))))
      (rows nil))
  (dolist (key-pair '((#x3fffa9 . #xa9) (#x110000 . #xfffd)))
    (let* ((code (car key-pair))
           (scalar-key (cdr key-pair))
           (base (copy-syntax-table))
           (property (copy-syntax-table)))
      ;; Full-code and collapsed scalar keys deliberately disagree.
      (modify-syntax-entry scalar-key "." base)
      (modify-syntax-entry code "w" base)
      (modify-syntax-entry scalar-key "w" property)
      (modify-syntax-entry code "." property)
      (with-temp-buffer
        (set-syntax-table base)
        (dolist (mode '(supplied-multibyte computed-multibyte computed-unibyte))
          (dolist (with-properties '(nil t))
            (dolist (parse-sexp-lookup-properties '(nil t))
              (dolist (pattern '("\\w" "a\\b" "a\\B" "a\\<" "a\\>"
                                 "a\\_<" "a\\_>" "a\\w" "\\w\\b"))
                ;; Never reuse mutated property trees or string-position cache
                ;; identities between independent match observations.
                (let* ((source (funcall make-source code mode))
                       (read-symbol-shorthands nil)
                       (case-fold-search nil))
                  (when with-properties
                    (put-text-property 0 (length source) 'syntax-table property source))
                  (set-match-data '(11 12))
                  (let ((result
                         (condition-case err
                             (list 'ok (string-match pattern source 0) (match-data))
                           ;; Avoid printing the possibly supplied-count source
                           ;; embedded in error args; retain signal and registers.
                           (error (list 'signal (car err) (match-data))))))
                    (push (list code scalar-key mode with-properties
                                parse-sexp-lookup-properties pattern
                                (length source) (string-bytes source)
                                (multibyte-string-p source)
                                (append (string-as-unibyte source) nil)
                                result)
                          rows))))))))))
  (nreverse rows))
