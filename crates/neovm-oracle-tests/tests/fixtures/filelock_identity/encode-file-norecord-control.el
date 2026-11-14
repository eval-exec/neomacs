(let* ((saved (symbol-function 'make-lock-file-name))
       (encoded-source (string-as-multibyte "/threadsig-absent-lock-norecord"))
       (file-name-coding-system 'utf-8-unix)
       (default-file-name-coding-system nil)
       (last-coding-system-used 'threadsig-sentinel))
  (unwind-protect
      (progn
        (fset 'make-lock-file-name (lambda (_file) encoded-source))
        (file-locked-p "/threadsig-norecord-original")
        last-coding-system-used)
    (fset 'make-lock-file-name saved)))
