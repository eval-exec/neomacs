
(progn
  (let ((buf (generate-new-buffer " test-mb-proc-xxx"))))
    (with-current-buffer buf
      (let ((proc (start-process "test-mb-xxx" buf "printf" "caf\xc3\xa9 r\xc3\xa9sum\xc3\xa9")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (buffer-string))
              (len (length (buffer-string))))
          (kill-buffer buf)
          (list (string-match "caf" output)
                (string-match "r" output)
                (>= len 5)
                (= (length output) (+ (length output) 0)))))))
