;;; 05-thread-yield-handoff.el --- in-flight thread-blocked payload -*- lexical-binding: t -*-
;;; expect: (thread-probe-done (blocked datum) (cleanup ran))

;; DIVERGENCES.md 162. `Flow::ThreadBlocked' carries the object being waited on
;; and the forms to re-dispatch on resume, up the Rust stack, through the same
;; `unbind_to' that runs `unwind-protect' cleanups. Yielding from a thread is
;; the only way to produce one from Lisp.
;;
;; What this asserts is deliberately narrow, because `thread-yield' in a
;; non-main thread does not currently resume the rest of the body (measured on
;; the pre-fix binary too, so it is pre-existing and not this entry's: see
;; DIVERGENCES.md 162 "Found and NOT fixed"). It asserts that the datum built
;; BEFORE the yield is still intact after the join, that the cleanup ran and
;; built a live datum of its own, and that the process exited 0 -- all with a
;; full collection at every safe point in between.
;;
;; `thread-join' and nothing else: a `(while (thread-live-p ...) (thread-yield))'
;; spin loop in the MAIN thread never terminates under this scheduler and burns
;; a core, which is a hang, not a probe.

(defvar gc-stress-sink nil)
(defvar gc-stress-before-yield nil)
(defvar gc-stress-cleanup nil)

(thread-join
 (make-thread
  (lambda ()
    (unwind-protect
        (progn
          (setq gc-stress-before-yield
                (list (intern (concat "blocked" ""))
                      (intern (concat "datum" ""))))
          (setq gc-stress-sink (make-list 256 'churn))
          (thread-yield))
      (setq gc-stress-cleanup
            (list (intern (concat "cleanup" ""))
                  (intern (concat "ran" ""))))))
  "gc-stress-probe"))

(setq gc-stress-sink (make-list 4096 'churn))
(prin1 (list 'thread-probe-done gc-stress-before-yield gc-stress-cleanup))
(terpri)
