;;; mir-buffer-read-bench.el --- Checked buffer reads with arithmetic -*- lexical-binding: t; -*-

(require 'bytecomp)

(defalias 'neovm-mir-read-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (setq sum (+ sum n (point)))
         (setq n (1- n)))
       sum))))

(defalias 'neovm-mir-read-polynomial
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (setq sum (+ sum (* n n) (* 3 n) (point)))
         (setq n (1- n)))
       sum))))

(with-temp-buffer
  (insert "abcdefg")
  (goto-char 7)
  (let ((n 100000))
    (dolist (case `((neovm-mir-read-sum . ,(+ (/ (* n (1+ n)) 2) (* 7 n)))
                    (neovm-mir-read-polynomial
                     . ,(+ (/ (* n (1+ n) (1+ (* 2 n))) 6)
                            (/ (* 3 n (1+ n)) 2) (* 7 n)))))
      ;; Heat ENTRY as well as the back edge: OSR currently uses the baseline
      ;; lowering, so one long warmup alone would not measure the MIR tier.
      (dotimes (_ 5000) (funcall (car case) 1))
      (dotimes (_ 10)
        (unless (= (funcall (car case) n) (cdr case)) (error "warmup mismatch")))
      (let (times)
        (dotimes (_ 9)
          (let* ((start (current-time))
                 (result (funcall (car case) n))
                 (elapsed (float-time (time-subtract (current-time) start))))
            (unless (= result (cdr case)) (error "result mismatch: %S" result))
            (push elapsed times)))
        (princ (format "BENCH %s n=%d median_us=%.1f result=%d\n"
                       (car case) n (* 1e6 (nth 4 (sort times #'<))) (cdr case)))))))

;;; mir-buffer-read-bench.el ends here
