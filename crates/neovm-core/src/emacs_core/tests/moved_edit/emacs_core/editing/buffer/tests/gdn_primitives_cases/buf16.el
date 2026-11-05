(condition-case err
    (with-temp-buffer (insert "abc") (list (insert-buffer-substring nil 1 2) (buffer-string)))
  (error err))
