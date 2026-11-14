(let* ((threadsig-original (copy-sequence "/threadsig-identity:probe"))
       (threadsig-marker (vector (list 'payload 17)))
       (threadsig-observed nil)
       (threadsig-operation 'lock-file))
  (set-text-properties 0 (length threadsig-original) nil threadsig-original)
  (let* ((file-name-handler-alist
          (list
           (cons "\\`/threadsig-identity:"
                 (lambda (operation &rest args)
                   (let ((file (car args)))
                     (setq threadsig-observed
                           (list (eq file threadsig-original)
                                 (eq operation threadsig-operation)
                                 (null (text-properties-at 0 file))))
                     (aset file 0 ?X)
                     (put-text-property 0 1 'threadsig-mutated 'yes file)
                     (setq threadsig-observed
                           (append threadsig-observed
                                   (list (eq (get-text-property 0 (quote threadsig-mutated) file) (quote yes)))))
                     'handler-token)))))
         (result (funcall threadsig-operation threadsig-original)))
    (list result threadsig-observed
          (aref threadsig-original 0)
          (get-text-property 0 'threadsig-mutated threadsig-original))))
