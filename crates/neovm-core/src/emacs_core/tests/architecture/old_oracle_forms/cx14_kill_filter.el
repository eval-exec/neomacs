

(let (fired)
  (let ((buf (get-buffer-create " *neo-cx14-pk*"))))
    (with-current-buffer buf (insert "preexisting"))
    (let ((p (make-process :name "neo-cx14-pk" :command '("echo" "output")
                           :buffer buf
                           :filter (lambda (proc msg) (setq fired :filter-fired)
                                     (with-current-buffer (process-buffer proc) (insert msg))))))
      (neovm--oracle-settle-process p 600)
      (let ((content (with-current-buffer buf (buffer-string))))
        (kill-buffer buf)
        (list fired content (process-buffer p)))))
