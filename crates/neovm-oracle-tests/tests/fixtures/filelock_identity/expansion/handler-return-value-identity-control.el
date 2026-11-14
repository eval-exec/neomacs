;; UNEXECUTED SOURCE-ONLY PROBE. No observed result or regression grade.
(let* ((name (copy-sequence "/threadsig-expand-return:leaf"))
       (returned (copy-sequence "/threadsig-expanded-return"))
       (marker (vector 'payload)))
  (put-text-property 0 1 'threadsig-payload marker returned)
  (let* ((file-name-handler-alist
          (list (cons "\\`/threadsig-expand-return:"
                      (lambda (_operation _name _directory)
                        (garbage-collect)
                        returned))))
         (result (expand-file-name name nil)))
    (list (eq result returned)
          (eq (get-text-property 0 'threadsig-payload result) marker)) ))
