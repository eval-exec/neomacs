(let* ((create-lockfiles t)
       (file-name-coding-system 'no-conversion)
       (dir (make-temp-file "threadsig-weak-original-" t))
       (file (expand-file-name "visited" dir))
       (lock-name (string-as-unibyte (expand-file-name "lock" dir)))
       (buffer (generate-new-buffer " *threadsig-weak-original*"))
       (weak (make-hash-table :test 'eq :weakness 'key))
       (handler (make-symbol "threadsig-weak-original-verify"))
       (old-make (symbol-function 'make-lock-file-name))
       separate observation)
  (unwind-protect
      (progn
        (let ((file-name-handler-alist nil)) (write-region "seed" nil file nil 'silent))
        (with-current-buffer buffer
          (setq buffer-file-name (copy-sequence file)
                buffer-file-truename (copy-sequence file))
          (setq separate (and (not (eq buffer-file-truename file))
                              (not (eq buffer-file-truename buffer-file-name))))
          (puthash buffer-file-truename 'original weak)
          (set-visited-file-modtime '(1 0 0 0))
          (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
        (fset 'make-lock-file-name (lambda (_expanded) lock-name))
        (fset handler
              (lambda (operation target)
                (unless (eq target buffer) (error "Unexpected visited buffer"))
                (with-current-buffer target (setq buffer-file-truename nil))
                (garbage-collect)
                (setq observation
                      (list operation (hash-table-count weak)
                            (null (buffer-local-value 'buffer-file-truename target))))
                t))
        (put handler 'operations '(verify-visited-file-modtime))
        (let ((file-name-handler-alist
               (list (cons (concat "\\`" (regexp-quote file) "\\'") handler))))
          ;; No Lisp lock-file argument or independent binding holds the
          ;; truename object: insertion reaches Flock_file from native editing.
          (with-current-buffer buffer (insert "x")))
        (unless observation (error "Native verification callback was not reached"))
        (list separate observation
              (with-current-buffer buffer (buffer-string))
              (not (null (file-symlink-p lock-name)))))
    (fset 'make-lock-file-name old-make)
    (fmakunbound handler)
    (setplist handler nil)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-file-name nil buffer-file-truename nil)
        (let ((inhibit-modification-hooks t)) (set-buffer-modified-p nil)))
      (kill-buffer buffer))
    (delete-directory dir t)))
