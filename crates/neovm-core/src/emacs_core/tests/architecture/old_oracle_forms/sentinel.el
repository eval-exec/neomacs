
(progn
  (defvar test-sentinel-log-xxx nil)
  (let ((buf (generate-new-buffer " test-sent-xxx"))))
    (with-current-buffer buf
      (let ((proc (start-process "test-sent-xxx" buf "echo" "sentinel-test")))
        (set-process-query-on-exit-flag proc nil)
        (set-process-sentinel proc
                              (lambda (proc event)
                                (push (format "event: %s" event) test-sentinel-log-xxx)))
        (while (process-live-p proc) (accept-process-output proc 1))
        (accept-process-output nil 0.1)
        (let ((output (buffer-string))
              (log test-sentinel-log-xxx))
          (kill-buffer buf)
          (list (string-match "sentinel-test" output)
                (>= (length log) 1)
                (stringp output))))))
