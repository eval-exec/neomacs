;;; -*- lexical-binding: t; -*-
;; Postlude-only rows assign the list inside ASV, after its entry-time check.
(setq probe-root (getenv "SHUTDOWN_PROBE_ROOT") probe-row (getenv "SHUTDOWN_LIST_ROW"))
(setq probe-artifact (getenv "SHUTDOWN_ARTIFACT") probe-secondary (getenv "SHUTDOWN_SECONDARY"))
(setq probe-log (getenv "SHUTDOWN_LOG"))
(setq probe-auto (concat probe-root "/postlude-autosave"))
(setq probe-virtual "/threadsig-postlude-list")
(setq create-lockfiles nil auto-save-list-file-name nil)
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(defun probe-list-handler (operation &rest _args)
  (cond
   ((eq operation 'delete-file)
    (probe-record "delete-file-handler")
    (error "delete handler positive control"))
   ((eq operation 'expand-file-name)
    (probe-record "expand-handler")
    (if (equal probe-row "postlude-expand-nul")
        (concat probe-artifact (string 0) "suffix")
      (if (equal probe-row "postlude-expand-escape")
          (throw 'probe-escape 'expanded-escape)
        (if (member probe-row '("postlude-missing" "postlude-directory"))
            probe-secondary
          probe-artifact))))
   (t (error "unexpected list handler operation"))))
(defun probe-save-handler (operation &rest _args)
  (unless (eq operation 'write-region) (error "unexpected save handler operation"))
  (if (equal probe-row "save-escape")
      (progn (probe-record "save-escape") (throw 'probe-escape 'saved-escape))
    (setq auto-save-list-file-name
          (if (equal probe-row "postlude-input-nul")
              (concat probe-virtual (string 0) "suffix")
            (if (member probe-row '("postlude-raw-bypass" "postlude-expand-function-cell"))
                probe-artifact
              probe-virtual)))
    ;; A changed expand-file-name function cell can affect Lisp logging
    ;; dependencies. Keep this isolated row's ASV callback allocation-free.
    (unless (equal probe-row "postlude-expand-function-cell") (probe-record "save-set-list"))
    nil))
(put 'probe-save-handler 'operations '(write-region))
(cond
 ((equal probe-row "nonstr-nil") (setq auto-save-list-file-name nil))
 ((equal probe-row "nonstr-t") (setq auto-save-list-file-name t))
 ((equal probe-row "nonstr-int") (setq auto-save-list-file-name 42))
 ((equal probe-row "nonstr-cons") (setq auto-save-list-file-name '(ignored)))
 ((member probe-row '("signal-term" "signal-abort"))
  (setq auto-save-list-file-name probe-artifact))
 ((member probe-row '("unintern-nil" "unintern-decoy"))
  (setq probe-original-symbol 'auto-save-list-file-name)
  (set probe-original-symbol probe-artifact)
  (unless (unintern probe-original-symbol obarray) (error "unintern failed"))
  (setq probe-decoy-symbol (intern "auto-save-list-file-name" obarray))
  (when (eq probe-original-symbol probe-decoy-symbol) (error "identity did not change"))
  (set probe-decoy-symbol (unless (equal probe-row "unintern-nil") probe-secondary))
  (probe-record
   (format "identity:distinct=%S:original-preserved=%S:replacement=%S"
           (not (eq probe-original-symbol probe-decoy-symbol))
           (equal (symbol-value probe-original-symbol) probe-artifact)
           (if (null (symbol-value probe-decoy-symbol)) 'nil 'decoy))))
 (t
  (setq file-name-handler-alist
        (list (cons (concat "\\`" (regexp-quote probe-auto) "\\'") 'probe-save-handler)))
  (set-buffer (get-buffer-create "postlude-save-buffer"))
  (setq buffer-auto-save-file-name probe-auto)
  (insert "dirty")
  (cond
   ((equal probe-row "postlude-raw-bypass")
    ;; Restrict operations so these rows do not count ASV's unrelated entry
    ;; expansion. The original full raw rows retain their TWO expansions.
    (put 'probe-list-handler 'operations '(delete-file))
    (push (cons (concat "\\`" (regexp-quote probe-artifact) "\\'") 'probe-list-handler)
          file-name-handler-alist)
    (condition-case nil (delete-file probe-artifact)
      (error (probe-record "delete-handler-control-caught")))
    (fset 'delete-file (lambda (&rest _args)
                         (probe-record "delete-function-cell")
                         (error "delete function cell positive control")))
    (condition-case nil (delete-file probe-artifact)
      (error (probe-record "delete-function-control-caught"))))
   ((equal probe-row "postlude-expand-function-cell")
    ;; Native write-region/ASV calls have only a write-region handler, so the
    ;; redefined Lisp expand-file-name cannot confound ASV handler fallback.
    (setq probe-native-expand (symbol-function 'expand-file-name))
    (fset 'expand-file-name (lambda (&rest _args) (error "expand function cell positive control")))
    (setq probe-expand-caught
          (condition-case nil (expand-file-name probe-artifact) (error t)))
    (fset 'expand-file-name probe-native-expand)
    (unless probe-expand-caught (error "replacement expand positive control failed"))
    (probe-record "expand-function-control-caught"))
   ((equal probe-row "postlude-input-nul")
    (put 'probe-list-handler 'operations '(expand-file-name))
    ;; This would match the NUL input too, if expansion called its handler
    ;; before rejecting filenamep. Any such dispatch adds unexpected events.
    (push (cons "\\`/threadsig-postlude-list" 'probe-list-handler) file-name-handler-alist)
    (unless (eq (condition-case condition
                    (expand-file-name (concat probe-virtual (string 0) "suffix"))
                  (wrong-type-argument
                   (and (eq (nth 1 condition) 'filenamep) 'rejected)))
                'rejected)
      (error "NUL input did not reject before handler"))
    (probe-record "input-nul-control-filenamep"))
   ((equal probe-row "postlude-expand-nul")
    (put 'probe-list-handler 'operations '(expand-file-name))
    (push (cons "\\`/threadsig-postlude-list\\'" 'probe-list-handler) file-name-handler-alist)
    (let* ((wanted (concat probe-artifact (string 0) "suffix"))
           (expanded (expand-file-name probe-virtual)))
      (unless (and (equal expanded wanted)
                   (= (string-bytes expanded) (string-bytes wanted))
                   (= (aref expanded (length probe-artifact)) 0))
        (error "NUL handler did not return exact prefix/NUL/suffix bytes"))
      (probe-record
       (format "expand-nul-control-exact=t:suffix-bytes=%S"
               (string-to-list (substring expanded (length probe-artifact)))))))
   ((not (equal probe-row "save-escape"))
    (put 'probe-list-handler 'operations '(expand-file-name))
    (push (cons "\\`/threadsig-postlude-list\\'" 'probe-list-handler) file-name-handler-alist)
    (if (equal probe-row "postlude-expand-escape")
        (unless (eq (catch 'probe-escape (expand-file-name probe-virtual)) 'expanded-escape)
          (error "escape handler positive control failed"))
      (unless (equal (expand-file-name probe-virtual)
                     (if (member probe-row '("postlude-missing" "postlude-directory"))
                         probe-secondary probe-artifact))
        (error "expansion handler positive control failed")))
    (probe-record "expand-control-ok")))))
(probe-record "ready")
(write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
(if (member probe-row '("signal-term" "signal-abort"))
    (while t (sleep-for 0.01))
  (while (not (file-exists-p (getenv "SHUTDOWN_GO"))) (sleep-for 0.01))
  (when (equal probe-row "postlude-expand-function-cell")
    ;; No Lisp logger or expanding fallback runs after this replacement.
    (fset 'expand-file-name (lambda (&rest _args) (error "expand function cell called"))))
  (if (member probe-row '("save-escape" "postlude-expand-escape"))
      (progn
        (probe-record (format "escaped:%S" (catch 'probe-escape (kill-emacs 0))))
        ;; The successful second exit must preserve the first escape artifact.
        (setq auto-save-list-file-name nil)
        (set-buffer-modified-p nil)
        (probe-record "second-kill")
        (kill-emacs 0))
    (if (equal probe-row "postlude-input-nul")
        (progn
          (unless (eq (condition-case condition (kill-emacs 0)
                        (wrong-type-argument
                         (and (eq (nth 1 condition) 'filenamep) 'caught)))
                      'caught)
            (error "postlude NUL input did not propagate filenamep"))
          (probe-record "postlude-input-nul-caught:filenamep")
          (setq auto-save-list-file-name nil)
          (set-buffer-modified-p nil)
          (probe-record "second-kill")
          (kill-emacs 0))
      (kill-emacs 0))))
