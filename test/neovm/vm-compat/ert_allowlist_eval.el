;;; ert_allowlist_eval.el --- Run an ERT allowlist in batch -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'ert)

(defun neovm--trim-line (s)
  (string-trim s))

(defun neovm--read-allowlist (path)
  (with-temp-buffer
    (insert-file-contents path)
    (let ((lines (split-string (buffer-string) "\n" t))
          out)
      (dolist (raw lines (nreverse out))
        (let ((line (neovm--trim-line raw)))
          (unless (or (string-empty-p line)
                      (string-prefix-p ";" line)
                      (string-prefix-p "#" line))
            (push line out)))))))

(defun neovm--escape-tsv (s)
  (replace-regexp-in-string
   "\n" "\\n"
   (replace-regexp-in-string "\t" "\\t" (format "%s" s) t t)
   t t))

(defun neovm--result-status (result)
  (cond
   ((cl-typep result 'ert-test-passed) "OK passed")
   ((cl-typep result 'ert-test-failed) "ERR failed")
   ((cl-typep result 'ert-test-aborted) "ERR aborted")
   (t "ERR unknown")))

(defun neovm--print-row (idx test-name status)
  (princ (format "%d\t%s\t%s\n" idx (neovm--escape-tsv test-name) status)))

(defun neovm--load-files-from-env ()
  (let ((load-files (getenv "NEOVM_ERT_LOAD_FILES")))
    (when (and load-files (not (string-empty-p load-files)))
      (dolist (path (split-string load-files ":" t))
        (load-file path)))))

(defun neovm--main ()
  (let ((allowlist (getenv "NEOVM_ERT_ALLOWLIST_FILE")))
    (unless (and allowlist (file-exists-p allowlist))
      (error "NEOVM_ERT_ALLOWLIST_FILE missing or unreadable"))

    (neovm--load-files-from-env)

    (let ((index 0))
      (dolist (name (neovm--read-allowlist allowlist))
        (setq index (1+ index))
        (condition-case _err
            (let* ((sym (intern-soft name))
                   (test (and sym (ert-get-test sym))))
              (if (null test)
                  (neovm--print-row index name "ERR missing-test")
                (let ((ert-quiet t))
                  (neovm--print-row index name (neovm--result-status (ert-run-test test))))))
          (error
           (neovm--print-row index name "ERR runner-error")))))))

(neovm--main)
