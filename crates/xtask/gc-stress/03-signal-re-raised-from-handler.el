;;; 03-signal-re-raised-from-handler.el --- signal payload across a handler -*- lexical-binding: t -*-
;;; expect: (my-probe-error "fresh datum" (1 2 3))

;; A `condition-case' handler that allocates and then RE-SIGNALS. The original
;; payload has to stay live across the handler body (arbitrary Lisp, many safe
;; points), and the re-raised one across the outer unwind. `define-error' with
;; a fresh condition also exercises the part of the pin that keeps the error
;; SYMBOL alive: `signal_or_quit' reads `error-conditions' off the symbol
;; OBJECT (GNU src/eval.c), so an uninterned or freshly interned symbol's
;; property cells survive only while something marks it.

(define-error 'my-probe-error "Probe")

(defvar gc-stress-sink nil)

(prin1
 (condition-case outer
     (condition-case inner
         (signal 'my-probe-error (list (concat "fresh" " " "datum") (list 1 2 3)))
       (my-probe-error
        (setq gc-stress-sink (make-list 512 'churn))
        (signal (car inner) (cdr inner))))
   (my-probe-error
    (setq gc-stress-sink (make-list 512 'churn))
    (cons (car outer) (cdr outer)))))
(terpri)
