(let* ((create-lockfiles nil)
       (file-name-handler-alist nil)
       (dir (make-temp-file "threadsig-write-visit-mutation-" t))
       (filename (copy-sequence (string-as-unibyte (expand-file-name "out-a" dir))))
       (mutated-name (copy-sequence (string-as-unibyte (expand-file-name "out-b" dir))))
       (visit-name (copy-sequence "/threadsig-write-visit:marker"))
       (visit-handler (make-symbol "threadsig-write-visit-expand"))
       (write-handler (make-symbol "threadsig-write-after-visit"))
       (buffer (generate-new-buffer " *threadsig-write-visit-mutation*"))
       events result)
  (unwind-protect
      (progn
        (fset visit-handler
              (lambda (operation &rest args)
                (cond
                 ((eq operation 'expand-file-name)
                  (setq events (cons (list 'visit-expand (eq (car args) visit-name)) events))
                  (aset filename (1- (length filename)) ?b)
                  visit-name)
                 ;; A native fallback may unlock VISIT.  Restrict this handler
                 ;; to expansion/unlocking; it never intercepts write-region.
                 ((eq operation 'unlock-file) nil)
                 (t (error "Unexpected VISIT handler operation")))))
        (put visit-handler 'operations '(expand-file-name unlock-file))
        (fset write-handler
              (lambda (operation &rest args)
                (setq events
                      (cons (list operation
                                  (eq (nth 2 args) filename)
                                  (string-equal (nth 2 args) mutated-name)
                                  (eq (nth 4 args) visit-name)
                                  (eq (nth 5 args) visit-name))
                            events))
                'threadsig-write-handler-marker))
        (put write-handler 'operations '(write-region))
        (with-current-buffer buffer
          (let ((default-directory (string-as-unibyte "/"))
                (file-name-handler-alist
                 (list (cons "\\`/threadsig-write-visit:" visit-handler)
                       (cons (concat "\\`" (regexp-quote mutated-name) "\\'") write-handler))))
            (setq result (write-region "x" nil filename nil visit-name)))
          (setq buffer-file-name nil buffer-file-truename nil))
        ;; This probes handler selection after VISIT expansion.  It does not
        ;; establish which native path a fallback write would consume.
        (list (nreverse events) result (string-equal filename mutated-name)))
    (fmakunbound visit-handler)
    (setplist visit-handler nil)
    (fmakunbound write-handler)
    (setplist write-handler nil)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-file-name nil buffer-file-truename nil)
        (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
      (kill-buffer buffer))
    (delete-directory dir t)))
