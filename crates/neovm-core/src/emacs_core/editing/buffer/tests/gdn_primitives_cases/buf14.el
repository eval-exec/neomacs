(condition-case err
    (with-temp-buffer (insert "abc") (setq buffer-read-only t) (list (delete-char 0) (buffer-string)))
  (error err))
