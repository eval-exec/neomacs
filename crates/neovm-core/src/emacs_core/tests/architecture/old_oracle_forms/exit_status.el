
(progn
  (let ((proc (start-process "test-exit-xxx" nil "sh" "-c" "exit 42"))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (process-exit-status proc)
          (= (process-exit-status proc) 42)
          (eq (process-status proc) 'exit)
          (null (process-live-p proc))))
