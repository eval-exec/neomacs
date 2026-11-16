
(progn
  (let ((buf (generate-new-buffer " test-args-xxx"))))
    (with-current-buffer buf
      (let ((proc (start-process "test-args-xxx" buf "printf" "%s-%s" "hello" "world")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (with-current-buffer buf (buffer-string))))
          (kill-buffer buf)
          (list (string= output "hello-world")
                (= (length output) 11)
                (stringp output))))))
