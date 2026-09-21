;;; mir-inline-float-bench.el --- Float helper call control -*- lexical-binding: t; -*-
(require 'bytecomp)
(defalias 'neovm-mir-inline-float-step (byte-compile (lambda (x) (+ x 0.5))))
(defalias 'neovm-mir-inline-float-loop
  (byte-compile
   (lambda (n x)
     (while (> n 0)
       (setq x (neovm-mir-inline-float-step x))
       (setq n (1- n)))
     x)))
(dotimes (_ 5000) (neovm-mir-inline-float-loop 1 0.5))
(defalias 'neovm-mir-inline-float-short
  (byte-compile (lambda (n)
    (let ((x 0.0))
      (dotimes (_ n) (setq x (neovm-mir-inline-float-loop 1 0.5))) x))))
(dotimes (_ 5000) (neovm-mir-inline-float-short 1))
(let ((n 20000) times)
  (dotimes (_ 5)
    (let* ((start (current-time)) (result (neovm-mir-inline-float-short n)))
      (unless (= result 1.0) (error "wrong float result"))
      (push (* 1e6 (float-time (time-subtract (current-time) start))) times)))
  (princ (format "FLOAT-SHORT median_us=%.1f\n" (nth 2 (sort times #'<)))))
