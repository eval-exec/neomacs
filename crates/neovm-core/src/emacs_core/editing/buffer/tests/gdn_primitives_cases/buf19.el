(condition-case err
    (let ((b (generate-new-buffer " o"))) (with-current-buffer b (insert "é中😀abcd")) (with-temp-buffer (insert "abcdefghijklmnop") (let ((m (with-current-buffer b (copy-marker 4)))) (prog1 (list (char-after m) (char-before m)) (kill-buffer b)))))
  (error err))
