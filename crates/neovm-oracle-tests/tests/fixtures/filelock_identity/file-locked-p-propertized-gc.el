(let* ((threadsig-original (copy-sequence "/threadsig-identity:probe"))
       (threadsig-marker (vector (list 'payload 17)))
       (threadsig-observed nil)
       (threadsig-operation 'file-locked-p))
  (put-text-property 0 1 (quote threadsig-payload) threadsig-marker threadsig-original)
  (let* ((file-name-handler-alist
          (list
           (cons "\\`/threadsig-identity:"
                 (lambda (operation &rest args)
                   (let ((file (car args)))
                     (setq threadsig-observed
                           (list (eq file threadsig-original)
                                 (eq operation threadsig-operation)
                                 (eq (get-text-property 0 (quote threadsig-payload) file) threadsig-marker)))
                     (aset file 0 ?X)
                     (put-text-property 0 1 'threadsig-mutated 'yes file)
                     (garbage-collect)
                     (setq threadsig-observed
                           (append threadsig-observed
                                   (list (eq (get-text-property 0 (quote threadsig-payload) file) threadsig-marker))))
                     'handler-token)))))
         (result (funcall threadsig-operation threadsig-original)))
    (list result threadsig-observed
          (aref threadsig-original 0)
          (get-text-property 0 'threadsig-mutated threadsig-original))))
