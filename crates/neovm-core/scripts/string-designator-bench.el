;;; string-designator-bench.el --- typed string operand cost -*- lexical-binding: t; -*-
;; Run baseline/candidate/GNU processes in alternating order on the same CPU.
;; NEOVM_JIT=0 selects the Neomacs interpreter control. Every sample is checked.
(require 'bytecomp)
(fset 'designator-lessp-target (symbol-function 'string-lessp))
(fset 'designator-equal-target (symbol-function 'string-equal))
(defun designator-lessp-loop (n a b)
  (let (answer)
    (dotimes (_ n) (setq answer (designator-lessp-target a b)))
    answer))
(defun designator-equal-loop (n a b)
  (let (answer)
    (dotimes (_ n) (setq answer (designator-equal-target a b)))
    answer))
(dolist (fn '(designator-lessp-loop designator-equal-loop))
  (byte-compile fn)
  (unless (byte-code-function-p (symbol-function fn))
    (error "Expected bytecode for %s" fn)))
(let ((materialized-a 'designator-materialized-a)
      (materialized-b 'designator-materialized-b)
      (exact-a (make-symbol (copy-sequence "abc")))
      (exact-b (make-symbol (copy-sequence "def"))))
  ;; Exercise both names supplied by Lisp and names materialized after reading.
  (symbol-name materialized-a)
  (symbol-name materialized-b)
  (dolist (case (list
                (list 'lessp-strings 'designator-lessp-loop "abc" "def" t)
                (list 'lessp-symbols 'designator-lessp-loop 'abc 'def t)
                (list 'lessp-materialized 'designator-lessp-loop
                      materialized-a materialized-b t)
                (list 'lessp-exact 'designator-lessp-loop exact-a exact-b t)
                (list 'lessp-mixed 'designator-lessp-loop 'abc "def" t)
                (list 'lessp-unibyte 'designator-lessp-loop
                      (unibyte-string 127) (unibyte-string 128) t)
                (list 'lessp-multibyte 'designator-lessp-loop "é" "ü" t)
                (list 'equal-strings 'designator-equal-loop
                      "abc" (copy-sequence "abc") t)
                (list 'equal-symbols 'designator-equal-loop 'abc 'abc t)
                (list 'equal-materialized 'designator-equal-loop
                      materialized-a materialized-a t)
                (list 'equal-exact 'designator-equal-loop exact-a exact-a t)
                (list 'equal-mixed 'designator-equal-loop 'abc "abc" t)))
    (let ((name (nth 0 case)) (fn (nth 1 case))
          (a (nth 2 case)) (b (nth 3 case)) (expected (nth 4 case))
          samples)
      ;; Warm entry dispatch too: a long OSR loop alone can hide entry costs.
      (dotimes (_ 5000) (funcall fn 1 a b))
      (dotimes (_ 10) (funcall fn 2000 a b))
      (dotimes (_ 7)
        (let* ((start (float-time))
               (value (funcall fn 500000 a b))
               (elapsed (- (float-time) start)))
          (unless (equal value expected)
            (error "Wrong result for %s: %S" name value))
          (push (* elapsed 1e6) samples)))
      (princ (format "BENCH %s n=500000 median_us=%.1f samples_us=%S\n"
                     name (nth 3 (sort (copy-sequence samples) #'<))
                     (nreverse samples))))))
