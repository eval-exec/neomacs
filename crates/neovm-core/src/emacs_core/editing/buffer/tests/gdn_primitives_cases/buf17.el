(condition-case err
    (with-temp-buffer (set-buffer-multibyte nil) (insert "abc\351") (subst-char-in-region 1 5 #x3fffe9 ?z) (subst-char-in-region 1 5 #x161 ?Z) (buffer-string))
  (error err))
