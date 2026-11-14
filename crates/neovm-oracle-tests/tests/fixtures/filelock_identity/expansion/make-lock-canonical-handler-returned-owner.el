;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((name (copy-sequence (string-as-unibyte "leaf")))
       (default-directory (copy-sequence (string-as-unibyte "/threadsig-lock-expand:dir/")))
       (expanded (copy-sequence (string-as-unibyte "/threadsig-lock-expanded:leaf")))
       (marker (vector 'payload))
       (old (symbol-function 'make-lock-file-name))
       events result)
  (put-text-property 0 1 'threadsig-payload marker expanded)
  (let ((file-name-handler-alist
         (list (cons "\\`/threadsig-lock-expand:"
                     (lambda (operation received-name received-directory)
                       (setq events (append events
                                            (list (list 'expand operation
                                                        (eq received-name name)
                                                        (eq received-directory default-directory)))))
                       (garbage-collect)
                       expanded)))))
    (unwind-protect
        (progn
          (fset 'make-lock-file-name
                (lambda (received-name)
                  (setq events (append events
                                       (list (list 'make (eq received-name expanded)
                                                   (eq received-name name)
                                                   (eq (get-text-property 0 'threadsig-payload received-name) marker)))))
                  (put-text-property 0 1 'threadsig-mutated 'yes received-name)
                  (garbage-collect)
                  nil))
          (setq result (file-locked-p name)))
      (fset 'make-lock-file-name old)))
  (list result events (get-text-property 0 'threadsig-mutated expanded)))
