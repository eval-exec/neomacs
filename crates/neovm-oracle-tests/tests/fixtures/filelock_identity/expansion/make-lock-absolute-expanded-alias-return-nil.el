;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((file-name-handler-alist nil)
       (default-directory (string-as-unibyte "/"))
       (name (copy-sequence (string-as-unibyte "/threadsig-lock-owner-leaf")))
       (marker (vector 'payload))
       (old (symbol-function 'make-lock-file-name))
       observed result)
  (put-text-property 0 1 'threadsig-payload marker name)
  (unwind-protect
      (progn
        (fset 'make-lock-file-name
              (lambda (received-name)
                (setq observed
                      (list (eq received-name name)
                            (eq (get-text-property 0 'threadsig-payload received-name) marker)))
                (aset received-name (1- (length received-name)) ?X)
                (put-text-property 0 1 'threadsig-mutated 'yes received-name)
                (garbage-collect)
                nil))
        (setq result (file-locked-p name)))
    (fset 'make-lock-file-name old))
  (list result observed (substring-no-properties name)
        (get-text-property 0 'threadsig-mutated name)))
