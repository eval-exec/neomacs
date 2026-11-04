(condition-case err
    (let ((b (generate-new-buffer " sw"))) (with-current-buffer b (insert "abcd") (goto-char 2)) (with-temp-buffer (insert "xy") (goto-char 2) (save-excursion (buffer-swap-text b) (goto-char 4)) (prog1 (list (point) (buffer-string)) (kill-buffer b))))
  (error err))
