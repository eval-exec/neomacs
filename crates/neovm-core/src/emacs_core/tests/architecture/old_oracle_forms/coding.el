
(progn
  (let ((buf (generate-new-buffer " test-coding-xxx"))))
    (with-current-buffer buf
      (set-buffer-file-coding-system 'utf-8)
      (let ((proc (start-process "test-coding-xxx" buf "echo" "test-output")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (buffer-string)))
          (kill-buffer buf)
          (list (string-match "test-output" output)
                (stringp output)
                (> (length output) 0))))))
