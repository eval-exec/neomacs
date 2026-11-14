;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((name (copy-sequence (string-as-unibyte "/threadsig-prefix/../threadsig-normalized:leaf")))
       (directory (string-as-unibyte "/"))
       (returned (copy-sequence (string-as-unibyte "/threadsig-handled-result")))
       (marker (vector 'payload))
       observed)
  (put-text-property 0 1 'threadsig-payload marker returned)
  (let* ((file-name-handler-alist
          (list (cons "\\`/threadsig-normalized:"
                      (lambda (operation received-name received-directory)
                        (setq observed (list operation (eq received-name name)
                                             (eq received-directory directory)
                                             (substring-no-properties received-name)))
                        (garbage-collect)
                        returned))))
         (result (expand-file-name name directory)))
    (list observed (eq result returned)
          (substring-no-properties result)
          (eq (get-text-property 0 'threadsig-payload result) marker))))
