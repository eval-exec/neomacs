;;; mir-cons-bench.el --- Temporary pairs in hot loops -*- lexical-binding: t; -*-

(require 'bytecomp)

(defalias 'neovm-mir-cons-car-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (setq sum (+ sum (car (cons n nil))))
         (setq n (1- n)))
       sum))))

(defalias 'neovm-mir-cons-pair-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (let ((pair (cons n (1- n))))
           (setq sum (+ sum (+ (car pair) (cdr pair)))))
         (setq n (1- n)))
       sum))))

(let ((n 200000))
  (dolist (case `((neovm-mir-cons-car-sum . ,(/ (* n (1+ n)) 2))
                  (neovm-mir-cons-pair-sum . ,(* n n))))
    ;; Reach entry compilation; a long loop alone warms the separate OSR tier.
    (dotimes (_ 5000) (funcall (car case) 1))
    (dotimes (_ 10)
      (unless (= (funcall (car case) n) (cdr case)) (error "warmup mismatch")))
    (let ((before-gcs gcs-done) times)
      (dotimes (_ 9)
        (let* ((start (current-time))
               (result (funcall (car case) n))
               (elapsed (float-time (time-subtract (current-time) start))))
          (unless (= result (cdr case)) (error "result mismatch: %S" result))
          (push elapsed times)))
      (princ (format "BENCH %s n=%d median_us=%.1f result=%d gcs=%d\n"
                     (car case) n (* 1e6 (nth 4 (sort times #'<)))
                     (cdr case) (- gcs-done before-gcs))))))

;;; mir-cons-bench.el ends here
