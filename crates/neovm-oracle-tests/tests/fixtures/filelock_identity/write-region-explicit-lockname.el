(let* ((file (make-temp-file "threadsig-native-write-"))
       (lockname (copy-sequence "relative-lock-key"))
       (handler (make-symbol "threadsig-write-lock-handler"))
       (events nil))
  (unwind-protect
      (progn
        (fset handler
              (lambda (operation &rest args)
                (let ((name (car args)))
                  (setq events (append events
                                       (list (list operation (eq name lockname)
                                                   (eq (aref name 0) ?/)))))
                  (aset name (1- (length name)) ?X)
                  (garbage-collect)
                  nil)))
        (put handler 'operations '(lock-file unlock-file))
        (with-temp-buffer
          (let ((file-name-handler-alist (list (cons "relative-lock-" handler)))
                (coding-system-for-write 'utf-8-unix))
            (write-region "x" nil file nil nil lockname)))
        (list events (aref lockname (1- (length lockname)))
              (with-temp-buffer (insert-file-contents file) (buffer-string))))
    (when (file-exists-p file) (delete-file file))
    (fmakunbound handler)
    (setplist handler nil)))
