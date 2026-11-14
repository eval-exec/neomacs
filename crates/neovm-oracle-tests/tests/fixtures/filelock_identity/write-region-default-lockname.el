(let* ((filename (copy-sequence "/threadsig-write:output"))
       (visit-name (copy-sequence "/threadsig-write:visit"))
       (handler (make-symbol "threadsig-write-handler"))
       (events nil))
  (unwind-protect
      (progn
        (put handler 'operations '(write-region))
        (dolist (visit-arg (list t visit-name))
          (let ((expected-lock (if (eq visit-arg t) filename visit-name)))
            (fset handler
                  (lambda (operation &rest args)
                    (setq events (append events
                                         (list (list (eq operation 'write-region)
                                                     (eq (nth 2 args) filename)
                                                     (eq (nth 5 args) expected-lock)))))
                    nil))
            (with-temp-buffer
              (let ((default-directory (string-as-unibyte "/"))
                    (file-name-handler-alist (list (cons "\\`/threadsig-write:" handler))))
                (write-region "x" nil filename nil visit-arg nil))
              (setq buffer-file-name nil buffer-file-truename nil))))
        events)
    (fmakunbound handler)
    (setplist handler nil)))
