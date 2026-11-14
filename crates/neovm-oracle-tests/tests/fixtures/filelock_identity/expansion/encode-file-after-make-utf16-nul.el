;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((file-name-handler-alist nil)
       (default-directory (string-as-unibyte "/"))
       (file-name-coding-system 'utf-16le)
       (default-file-name-coding-system nil)
       (lockname (concat "/threadsig-encoded-lock-" (string #x3bb)))
       (old (symbol-function 'make-lock-file-name))
       result)
  (unwind-protect
      (progn
        (fset 'make-lock-file-name (lambda (_name) lockname))
        (setq result
              (condition-case err
                  (list 'returned (file-locked-p "/threadsig-native-source"))
                (error (list 'caught (car err)
                             (and (eq (car err) 'wrong-type-argument) (cadr err)))))))
    (fset 'make-lock-file-name old))
  (list (multibyte-string-p lockname)
        (null (string-match-p (string 0) lockname)) result))
