;;; 04-load-boundary-eval-error.el --- EvalError across the load boundary -*- lexical-binding: t -*-
;;; expect: (error "datum from a loaded file")

;; DIVERGENCES.md 162's second half. A signal raised inside `load' crosses the
;; PUBLIC boundary type: the evaluator's `Flow` becomes an `EvalError`, and
;; `map_flow' used to move the payload out of the pinned `SignalData' and drop
;; the pin. Everything the loader does on the way out -- popping the read
;; cursor, unbinding `standard-input', running the `unwind-protect' the loaded
;; file installed -- allocates while that error is in a Rust local.

(defvar gc-stress-sink nil)

(let ((file (make-temp-file "neomacs-gc-stress" nil ".el")))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "(defvar gc-stress-sink nil)\n")
          (insert "(unwind-protect\n")
          (insert "    (error \"%s\" (concat \"datum from a \" \"loaded file\"))\n")
          (insert "  (setq gc-stress-sink (make-list 512 'churn)))\n"))
        (prin1
         (condition-case err
             (load file nil t t)
           (error
            (setq gc-stress-sink (make-list 512 'churn))
            (cons (car err) (cdr err)))))
        (terpri))
    (delete-file file)))
