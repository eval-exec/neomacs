

(let ((timer-fired nil)
      (env-val (let ((process-environment (cons "NEO_CX106=v" process-environment)))
                 (string-trim (shell-command-to-string "echo $NEO_CX106"))))
      (exit-code (let ((p (make-process :name "neo-cx106-mega-ec"
                                          :command '("sh" "-c" "exit 5"))))
                   (accept-process-output p 2)
                   (process-exit-status p))))
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (sit-for 0.01)
  (let ((buf (get-buffer-create " *neo-cx106-mega*"))))
    (with-current-buffer buf
      (buffer-enable-undo)
      (insert "Subprocess mega test buffer content")
      (put-text-property 1 10 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)))
    (let ((p (make-process :name "neo-cx106-mega-p"
                           :command '("sh" "-c" "printf 'SUB'")
                           :buffer buf)))
      (accept-process-output p 1)
      (sit-for 0.05))
    (let ((content (with-current-buffer buf (buffer-string))))
      (with-current-buffer buf
        (widen)
        (let ((state (list timer-fired env-val exit-code
                           content (length content)
                           (length (overlays-in 1 20))
                           (text-properties-at 1))))
          (undo)
          (kill-buffer buf)
          (list state (buffer-string))))))
