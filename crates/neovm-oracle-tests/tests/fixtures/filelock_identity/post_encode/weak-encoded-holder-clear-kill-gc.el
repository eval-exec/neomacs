(let* ((create-lockfiles t)
       (file-name-coding-system 'no-conversion)
       (dir (make-temp-file "threadsig-weak-encoded-" t))
       (file (expand-file-name "visited" dir))
       (lock-a (expand-file-name "lock-a" dir))
       (lock-b (expand-file-name "lock-b" dir))
       (buffer (generate-new-buffer " *threadsig-weak-encoded-source*"))
       (holder (generate-new-buffer " *threadsig-weak-encoded-holder*"))
       (weak (make-hash-table :test 'eq :weakness 'key))
       (handler (make-symbol "threadsig-weak-encoded-verify"))
       (old-make (symbol-function 'make-lock-file-name))
       observation)
  (unwind-protect
      (progn
        (let ((file-name-handler-alist nil)) (write-region "seed" nil file nil 'silent))
        (with-current-buffer buffer
          (setq buffer-file-name (copy-sequence file)
                buffer-file-truename (copy-sequence file))
          (set-visited-file-modtime '(1 0 0 0))
          (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
        (fset 'make-lock-file-name
              (lambda (_expanded)
                (with-current-buffer holder
                  (setq-local threadsig-held-lock-name
                              (copy-sequence (string-as-unibyte lock-a)))
                  (puthash threadsig-held-lock-name 'encoded weak)
                  threadsig-held-lock-name)))
        (fset handler
              (lambda (operation target)
                (unless (eq target buffer) (error "Unexpected visited buffer"))
                (with-current-buffer holder
                  (kill-local-variable 'threadsig-held-lock-name))
                (kill-buffer holder)
                (garbage-collect)
                (let ((first-count (hash-table-count weak)))
                  ;; Recover only through the weak observer, after collection.
                  ;; An encoded native owner must still keep the key alive.
                  (maphash (lambda (key _value)
                             (aset key (1- (length key)) ?b))
                           weak)
                  (garbage-collect)
                  (setq observation
                        (list operation first-count (hash-table-count weak)
                              (buffer-live-p holder))))
                t))
        (put handler 'operations '(verify-visited-file-modtime))
        (let ((file-name-handler-alist
               (list (cons (concat "\\`" (regexp-quote file) "\\'") handler))))
          (lock-file file))
        (unless observation (error "Native verification callback was not reached"))
        (list observation
              (not (null (file-symlink-p lock-a)))
              (not (null (file-symlink-p lock-b)))))
    (fset 'make-lock-file-name old-make)
    (fmakunbound handler)
    (setplist handler nil)
    (dolist (target (list buffer holder))
      (when (buffer-live-p target)
        (with-current-buffer target
          (setq buffer-file-name nil buffer-file-truename nil)
          (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
        (kill-buffer target)))
    (delete-directory dir t)))
