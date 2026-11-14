(let* ((threadsig-original (copy-sequence "/threadsig-identity:control"))
       (threadsig-marker (vector (list 'payload 17)))
       (threadsig-observed nil)
       (threadsig-handler
        (lambda (operation &rest args)
          (let ((file (car args)))
            (setq threadsig-observed
                  (list (eq file threadsig-original)
                        (eq operation 'lock-file)
                        (eq (get-text-property 0 'threadsig-payload file)
                            threadsig-marker)))
            (aset file 0 ?X)
            (put-text-property 0 1 'threadsig-mutated 'yes file)
            (garbage-collect)
            (setq threadsig-observed
                  (append threadsig-observed
                          (list (eq (get-text-property 0 'threadsig-payload file)
                                    threadsig-marker))))
            'handler-token))))
  (put-text-property 0 1 'threadsig-payload threadsig-marker threadsig-original)
  (let ((result (funcall threadsig-handler 'lock-file threadsig-original)))
    (list result threadsig-observed
          (aref threadsig-original 0)
          (get-text-property 0 'threadsig-mutated threadsig-original))))
