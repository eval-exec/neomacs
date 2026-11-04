(condition-case err
    (with-temp-buffer (insert "abc") (goto-char 2) (delete-char nil))
  (error err))
