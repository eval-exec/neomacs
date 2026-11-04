(condition-case err
    (with-temp-buffer (insert "abé") (subst-char-in-region 1 4 ?a ?é))
  (error err))
