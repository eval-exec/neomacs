;;; -*- lexical-binding: t; -*-
;; Raw subprocess fixture: file-backed observations survive kill-emacs.
(setq create-lockfiles nil auto-save-list-file-name nil)
(setq probe-log (getenv "SHUTDOWN_LOG"))
(setq probe-case (getenv "SHUTDOWN_UNLOCK_CASE"))
(setq probe-mutated nil probe-armed t probe-hooks 0 probe-sentinels 0)
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(defun probe-make (suffix)
  (let ((buffer (get-buffer-create (concat "shutdown-mutation-" suffix))))
    (with-current-buffer buffer
      (setq buffer-file-truename (concat "/shutdown-mutation-probe/" suffix))
      (insert "dirty"))
    buffer))
(setq probe-escape (member probe-case '("unlock-throw" "post-dying-state")))
(setq probe-indirect (member probe-case '("indirect-modify-after" "indirect-clear-after")))
(when probe-indirect
  ;; The base is visited before A, then edited after indirect B is killed.
  (setq probe-base (get-buffer-create "shutdown-mutation-base"))
  (with-current-buffer probe-base (insert "initial") (set-buffer-modified-p nil)))
(setq probe-a (probe-make "a"))
(unless probe-escape
  (setq probe-b (if probe-indirect
                    (make-indirect-buffer probe-base "shutdown-mutation-b")
                  (probe-make "b")))
  (when probe-indirect
    (with-current-buffer probe-b
      (setq buffer-file-truename "/shutdown-mutation-probe/b"))
    (when (equal probe-case "indirect-clear-after")
      (with-current-buffer probe-base (insert "before"))))
  (when (member probe-case '("kill-a-then-b" "kill-b-then-a"
                            "indirect-modify-after" "indirect-clear-after"))
    (setq probe-c (probe-make "c"))))
(defun probe-sentinel (process _event)
  (setq probe-sentinels (1+ probe-sentinels))
  (probe-record (concat "sentinel:" (process-name process))))
(defun probe-pipe (name)
  (make-pipe-process :name name :noquery t :sentinel #'probe-sentinel))
(when probe-escape
  (setq auto-save-list-file-name (getenv "SHUTDOWN_ARTIFACT"))
  (setq kill-emacs-hook
        (list (lambda ()
                (setq probe-hooks (1+ probe-hooks))
                (probe-record (format "kill-hook:%s" probe-hooks)))))
  (when (equal probe-case "post-dying-state")
    (setq probe-control (probe-pipe "positive"))
    (unless (process-live-p probe-control) (error "positive pipe not live"))
    (delete-process probe-control)
    (probe-record (format "positive:sentinels=%s" probe-sentinels))
    (setq probe-pending (probe-pipe "pending"))
    (unless (process-live-p probe-pending) (error "pending pipe not live"))))
(defun probe-handler (operation &rest args)
  (when (eq operation 'unlock-file)
    (let ((suffix (substring (car args) (length "/shutdown-mutation-probe/"))))
      (probe-record
       (cond
        ((equal probe-case "post-dying-state")
         (format "unlock:%s:hooks=%s:sentinels=%s:pending-live=%S"
                 suffix probe-hooks probe-sentinels (process-live-p probe-pending)))
        (probe-escape (format "unlock:%s:hooks=%s" suffix probe-hooks))
        ((and (equal suffix "b") (not (buffer-live-p probe-b))) "unlock:b:dead")
        (t (concat "unlock:" suffix))))
      (when (and probe-escape probe-armed)
        (setq probe-armed nil)
        (throw 'probe-unlock-escape 'escaped))
      (when (and (not probe-mutated) (equal suffix "a") (not probe-escape))
        (setq probe-mutated t)
        (cond
         ((equal probe-case "append") (probe-make "c"))
         ((equal probe-case "rename-later")
          (with-current-buffer probe-b
            (setq buffer-file-truename "/shutdown-mutation-probe/b-renamed")))
         ((equal probe-case "clear-later")
          (with-current-buffer probe-b (set-buffer-modified-p nil)))
         ((equal probe-case "bury-current") (bury-buffer probe-a))
         ((equal probe-case "kill-current") (kill-buffer probe-a))
         ((equal probe-case "move-then-kill")
          (switch-to-buffer probe-b) (kill-buffer probe-a))
         ((equal probe-case "kill-then-move")
          (kill-buffer probe-a) (switch-to-buffer probe-b))
         ((equal probe-case "kill-b-then-a")
          (kill-buffer probe-b) (kill-buffer probe-a))
         ((member probe-case '("kill-a-then-b" "indirect-modify-after" "indirect-clear-after"))
          (kill-buffer probe-a) (kill-buffer probe-b)
          (cond
           ((equal probe-case "indirect-modify-after")
            (with-current-buffer probe-base (insert "after")))
           ((equal probe-case "indirect-clear-after")
            (with-current-buffer probe-base (set-buffer-modified-p nil)))))
         (t (error "Unknown mutation case: %s" probe-case))))))
  nil)
(setq file-name-handler-alist '(("\\`/shutdown-mutation-probe/" . probe-handler)))
(if probe-escape
    (progn
      (probe-record (format "list-before:%S" (file-exists-p auto-save-list-file-name)))
      (write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
      (while (not (file-exists-p (getenv "SHUTDOWN_GO"))) (sleep-for 0.01))
      (setq probe-result (catch 'probe-unlock-escape (kill-emacs 23) 'unexpected-return))
      (probe-record
       (if (equal probe-case "post-dying-state")
           (format "after-catch:%S:list=%S:hooks=%s:sentinels=%s"
                   probe-result (file-exists-p auto-save-list-file-name) probe-hooks probe-sentinels)
         (format "after-catch:%S:list=%S:hooks=%s"
                 probe-result (file-exists-p auto-save-list-file-name) probe-hooks)))
      (when (equal probe-case "post-dying-state")
        (setq probe-after (probe-pipe "after-escape"))
        (unless (process-live-p probe-after) (error "after-escape pipe not live"))
        (delete-process probe-after)
        (probe-record (format "after-escape:sentinels=%s" probe-sentinels)))
      (probe-record "second-kill")
      (kill-emacs 0))
  (kill-emacs 0))
