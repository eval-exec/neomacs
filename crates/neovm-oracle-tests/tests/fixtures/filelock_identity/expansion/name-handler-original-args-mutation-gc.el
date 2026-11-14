;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((name (copy-sequence (string-as-unibyte "/threadsig-expand-name:leaf")))
       (directory (copy-sequence (string-as-unibyte "/threadsig-directory/")))
       (marker (vector 'payload))
       observed)
  (put-text-property 0 1 'threadsig-payload marker name)
  (let* ((file-name-handler-alist
          (list (cons "\\`/threadsig-expand-name:"
                      (lambda (operation received-name received-directory)
                        (setq observed
                              (list operation (eq received-name name)
                                    (eq received-directory directory)
                                    (eq (get-text-property 0 'threadsig-payload received-name) marker)))
                        (aset received-name (1- (length received-name)) ?X)
                        (put-text-property 0 1 'threadsig-mutated 'yes received-name)
                        (aset received-directory (1- (length received-directory)) ?Y)
                        (garbage-collect)
                        received-name))))
         (result (expand-file-name name directory)))
    (list observed (eq result name)
          (substring-no-properties name)
          (substring-no-properties directory)
          (get-text-property 0 'threadsig-mutated name))))
