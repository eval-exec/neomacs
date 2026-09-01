;;; 02-throw-through-unwind-protect.el --- in-flight throw payload -*- lexical-binding: t -*-
;;; expect: ((thrown datum) (thrown datum) (thrown datum))

;; DIVERGENCES.md 162. `throw' unwinds through the same machinery `signal'
;; does: every frame runs `unbind_to', which executes `unwind-protect' cleanup
;; forms and variable watchers -- arbitrary Lisp, so allocation-bearing safe
;; points. GNU is safe for free here (`unwind_to_catch' longjmps with the value
;; on the C stack, which `mark_stack' scans conservatively); this collector is
;; precise, so `Flow::Throw''s payload has to be a seeded root or `catch'
;; returns a free-list cell.
;;
;; Three shapes, so the pin is exercised through more than one unwind: a plain
;; allocating cleanup, a nested `let' whose unbind runs a variable watcher, and
;; a cleanup that itself conses a large structure.

(defvar gc-stress-watched nil)
(add-variable-watcher
 'gc-stress-watched
 (lambda (&rest _) (setq gc-stress-sink (make-list 64 'w))))
(defvar gc-stress-sink nil)

(defun gc-stress-fresh-datum ()
  (list (intern (concat "thrown" "")) (intern (concat "datum" ""))))

(prin1
 (list
  ;; 1. allocating cleanup
  (catch 'probe
    (unwind-protect
        (throw 'probe (gc-stress-fresh-datum))
      (setq gc-stress-sink (make-list 256 'x))))
  ;; 2. cleanup plus a let-binding whose unbind fires a watcher
  (catch 'probe
    (let ((gc-stress-watched 1))
      (unwind-protect
          (throw 'probe (gc-stress-fresh-datum))
        (setq gc-stress-sink (make-list 256 'y)))))
  ;; 3. nested catch: the inner throw crosses two unwind boundaries
  (catch 'outer
    (catch 'inner
      (unwind-protect
          (unwind-protect
              (throw 'outer (gc-stress-fresh-datum))
            (setq gc-stress-sink (make-list 128 'z)))
        (setq gc-stress-sink (make-list 128 'w)))))))
(terpri)
