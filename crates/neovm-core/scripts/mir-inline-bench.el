;;; mir-inline-bench.el --- Calls eliminated inside loops -*- lexical-binding: t; -*-
(require 'bytecomp)
(defalias 'neovm-mir-inline-step (byte-compile (lambda (n) (1- n))))
(defalias 'neovm-mir-inline-id (byte-compile (lambda (n) n)))
(defalias 'neovm-mir-inline-step-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (setq sum (+ sum n))
         (setq n (neovm-mir-inline-step n)))
       sum))))
(defalias 'neovm-mir-inline-id-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (setq sum (+ sum (neovm-mir-inline-id n)))
         (setq n (1- n)))
       sum))))
(let ((n 200000))
  (dolist (fn '(neovm-mir-inline-step-sum neovm-mir-inline-id-sum))
    (let ((expected (/ (* n (1+ n)) 2)) times)
      (dotimes (_ 5000) (funcall fn 1))
      (dotimes (_ 10) (unless (= (funcall fn n) expected) (error "warmup mismatch")))
      (dotimes (_ 9)
        (let* ((start (current-time)) (result (funcall fn n))
               (elapsed (float-time (time-subtract (current-time) start))))
          (unless (= result expected) (error "result mismatch"))
          (push elapsed times)))
      (princ (format "BENCH %s n=%d median_us=%.1f result=%d\n"
                     fn n (* 1e6 (nth 4 (sort times #'<))) expected)))))
