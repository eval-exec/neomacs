;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((name (copy-sequence (string-as-unibyte "leaf")))
       (directory (copy-sequence (string-as-unibyte "relative")))
       (base (copy-sequence (string-as-unibyte "/threadsig-recursive-base:dir/")))
       (expanded-directory (copy-sequence (string-as-unibyte "/threadsig-recursive-result:dir/")))
       (default-directory base)
       (marker (vector 'payload))
       events)
  (put-text-property 0 1 'threadsig-payload marker expanded-directory)
  (let* ((file-name-handler-alist
          (list
           (cons "\\`/threadsig-recursive-base:"
                 (lambda (operation received-name received-directory)
                   (setq events (append events
                                        (list (list 'inner operation
                                                    (eq received-name directory)
                                                    (eq received-directory base)))))
                   (aset name (1- (length name)) ?X)
                   (put-text-property 0 1 'threadsig-mutated 'yes name)
                   (garbage-collect)
                   expanded-directory))
           (cons "\\`/threadsig-recursive-result:"
                 (lambda (operation received-name received-directory)
                   (setq events (append events
                                        (list (list 'outer operation
                                                    (eq received-name name)
                                                    (eq received-directory expanded-directory)
                                                    (substring-no-properties received-name)
                                                    (eq (get-text-property 0 'threadsig-payload received-directory) marker)))))
                   (garbage-collect)
                   received-name))))
         (result (expand-file-name name directory)))
    (list events (eq result name) (substring-no-properties name)
          (get-text-property 0 'threadsig-mutated name))))
