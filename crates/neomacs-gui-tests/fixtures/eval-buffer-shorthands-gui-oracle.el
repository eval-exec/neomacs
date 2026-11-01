;;; eval-buffer-shorthands-gui-oracle.el --- GNU/Neomacs eval-buffer oracle -*- lexical-binding: t -*-
;;
;; One `(NAME . STATE)` line per case, written to the path named by
;; NEOMACS_EVAL_BUFFER_ORACLE_RESULT. The test runs this fixture under both
;; editors on one X11 display and compares the files line by line.
;;
;; The contract pinned here, from GNU 31.1: the reader consults the special
;; variable `read-symbol-shorthands` while reading (src/lread.c:4956-5010), so
;; `eval-buffer` and `eval-region` rewrite symbols according to a
;; buffer-local or dynamically bound value. Neomacs previously wired
;; shorthands only into the file-load path (issue #658).

(defvar neo-eval-buffer-oracle-results nil)

(defun neo-eval-buffer-oracle--record (name thunk)
  (push (cons name (funcall thunk))
        neo-eval-buffer-oracle-results))

(neo-eval-buffer-oracle--record
 'eval-buffer-buffer-local
 (lambda ()
   (with-temp-buffer
     (setq-local read-symbol-shorthands '(("oi-" . "gptel-openrouter-")))
     (insert "(setq oi-gui-local 11)")
     (eval-buffer)
     (list (boundp 'gptel-openrouter-gui-local)
           (boundp 'oi-gui-local)
           (and (boundp 'gptel-openrouter-gui-local)
                (symbol-value 'gptel-openrouter-gui-local))))))

(neo-eval-buffer-oracle--record
 'eval-region-dynamic
 (lambda ()
   (let ((read-symbol-shorthands '(("zz-" . "longzz-"))))
     (with-temp-buffer
       (insert "(setq zz-gui-dynamic 22)")
       (eval-region (point-min) (point-max))
       (list (boundp 'longzz-gui-dynamic)
             (boundp 'zz-gui-dynamic)
             (and (boundp 'longzz-gui-dynamic)
                  (symbol-value 'longzz-gui-dynamic)))))))

(let ((path (getenv "NEOMACS_EVAL_BUFFER_ORACLE_RESULT")))
  (with-temp-file path
    (dolist (result (reverse neo-eval-buffer-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(kill-emacs 0)
