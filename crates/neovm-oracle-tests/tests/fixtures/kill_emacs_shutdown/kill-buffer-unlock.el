;;; -*- lexical-binding: t; -*-
;; The observation is ordinary kill-buffer, before any batch shutdown.
(setq create-lockfiles nil auto-save-list-file-name nil)
(setq probe-log (getenv "SHUTDOWN_LOG"))
(setq probe-case (getenv "KILL_BUFFER_UNLOCK_CASE") probe-armed t)
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(if (equal probe-case "indirect-throw")
    (progn
      (setq probe-base (get-buffer-create "inner-unlock-base"))
      (with-current-buffer probe-base (insert "initial") (set-buffer-modified-p nil))
      (setq probe-target (make-indirect-buffer probe-base "inner-unlock-target"))
      ;; Modify the base after making the indirect to prove live shared ticks.
      (with-current-buffer probe-base (insert "after-indirect")))
  (setq probe-target (get-buffer-create "inner-unlock-target")))
(with-current-buffer probe-target
  (unless (equal probe-case "indirect-throw") (insert "dirty"))
  ;; These deliberately disagree: native unlock_buffer uses truename.
  (setq buffer-file-name "/inner-visible-probe/filename")
  (setq buffer-file-truename "/inner-unlock-probe/truename")
  (cond
   ((equal probe-case "unmodified") (set-buffer-modified-p nil))
   ((equal probe-case "nil-truename") (setq buffer-file-truename nil))))
(when (equal probe-case "current-reselect")
  (setq probe-third (get-buffer-create "inner-unlock-third")))
(setq probe-pipe
      (make-pipe-process
       :name "inner-unlock-pipe" :buffer probe-target :noquery t
       :sentinel (lambda (_process _event)
                   (probe-record (format "sentinel:target-live=%S" (buffer-live-p probe-target))))))
(unless (and (buffer-live-p probe-target) (process-live-p probe-pipe))
  (error "target/process prerequisite missing"))
(when (and (equal probe-case "indirect-throw") (not (buffer-modified-p probe-target)))
  (error "indirect does not observe modified base"))
(defun probe-unlock-handler (operation &rest args)
  (when (eq operation 'unlock-file)
    (probe-record
     (format "unlock:%s:armed=%S:target-live=%S:current-target=%S:pipe-live=%S"
             (car args) probe-armed (buffer-live-p probe-target)
             (eq (current-buffer) probe-target) (and (process-live-p probe-pipe) t)))
    (when probe-armed
      (cond
       ((member probe-case '("throw" "current-throw" "indirect-throw"))
        (setq probe-armed nil)
        (throw 'inner-unlock-escape 'escaped))
       ((equal probe-case "file-error")
        (setq probe-armed nil)
        (signal 'file-error '("handler-boom")))
       ((equal probe-case "kill-target")
        (setq probe-armed nil)
        (kill-buffer probe-target))))
    (when (equal probe-case "current-reselect") (set-buffer probe-third))
    'ignored-handler-result))
(setq file-name-handler-alist '(("\\`/inner-unlock-probe/" . probe-unlock-handler)))
(when (equal probe-case "function-cell")
  (fset 'unlock-file (lambda (&rest _args) (probe-record "REPLACED-UNLOCK-FILE"))))
(when (member probe-case '("current" "current-throw" "current-reselect"))
  (set-buffer probe-target))
(setq probe-result
      (cond
       ((member probe-case '("throw" "current-throw" "indirect-throw"))
        (catch 'inner-unlock-escape (kill-buffer probe-target)))
       ((equal probe-case "file-error")
        (condition-case err (kill-buffer probe-target)
          (file-error (list 'caught (car err)))))
       (t (kill-buffer probe-target))))
(probe-record
 (format "after-first:result=%S:target-live=%S:pipe-live=%S:current-target=%S"
         probe-result (buffer-live-p probe-target) (and (process-live-p probe-pipe) t)
         (eq (current-buffer) probe-target)))
(when (equal probe-case "current-reselect")
  (probe-record (format "after-selection:third=%S" (eq (current-buffer) probe-third))))
(when (member probe-case '("throw" "current-throw" "indirect-throw" "file-error"))
  (setq probe-result (kill-buffer probe-target))
  (probe-record
   (format "after-second:result=%S:target-live=%S:pipe-live=%S"
           probe-result (buffer-live-p probe-target) (and (process-live-p probe-pipe) t))))
