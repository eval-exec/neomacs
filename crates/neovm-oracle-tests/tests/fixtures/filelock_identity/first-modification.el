(let* ((threadsig-buffer (generate-new-buffer " *threadsig-value-origin*"))
       (threadsig-original (copy-sequence "/threadsig-buffer:probe"))
       (threadsig-marker (vector (list 'payload 29)))
       (threadsig-events nil)
       (threadsig-result nil))
  (unwind-protect
      (with-current-buffer threadsig-buffer
        (let ((inhibit-modification-hooks t))
          (set-buffer-modified-p nil))
        (put-text-property 0 1 'threadsig-payload threadsig-marker threadsig-original)
        (setq buffer-file-name (copy-sequence "/threadsig-buffer:visited")
              buffer-file-truename threadsig-original)
        (let ((inhibit-modification-hooks nil)
              (file-name-handler-alist
               (list
                (cons "\\`/threadsig-buffer:"
                      (lambda (operation &rest args)
                        (let ((file (car args)))
                          (setq threadsig-events
                                (append threadsig-events
                                        (list
                                         (list operation
                                               (eq file (buffer-local-value 'buffer-file-truename threadsig-buffer))
                                               (eq (get-text-property 0 'threadsig-payload file) threadsig-marker)))))
                          (aset file (1- (length file)) ?X)
                          (put-text-property 0 1 'threadsig-mutated 'yes file)
                          (garbage-collect)
                          nil))))))
          (insert "x"))
        (setq threadsig-result
              (list threadsig-events
                    (aref threadsig-original (1- (length threadsig-original)))
                    (get-text-property 0 'threadsig-mutated threadsig-original)
                    (buffer-live-p threadsig-buffer))))
    (when (buffer-live-p threadsig-buffer)
      (with-current-buffer threadsig-buffer
        (setq buffer-file-name nil buffer-file-truename nil)
        (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
      (kill-buffer threadsig-buffer)))
  threadsig-result)
