;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((file-name-handler-alist nil)
       (default-directory (string-as-unibyte "/"))
       (file-name-coding-system nil)
       (default-file-name-coding-system nil)
       (name (copy-sequence (string-as-unibyte "/threadsig-native-alias-leaf")))
       (old (symbol-function 'make-lock-file-name))
       observed result)
  (unwind-protect
      (progn
        (fset 'make-lock-file-name
              (lambda (received-name)
                (setq observed (eq received-name name))
                (aset received-name (1- (length received-name)) 0)
                (put-text-property 0 1 'threadsig-mutated 'yes received-name)
                (garbage-collect)
                received-name))
        (setq result
              (condition-case err
                  (list 'returned (file-locked-p name))
                (error (list 'caught (car err)
                             (and (eq (car err) 'wrong-type-argument) (cadr err)))))))
    (fset 'make-lock-file-name old))
  (list observed (not (null (string-match-p (string 0) name)))
        (get-text-property 0 'threadsig-mutated name) result))
