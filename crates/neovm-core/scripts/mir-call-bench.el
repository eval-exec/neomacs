;;; mir-call-bench.el --- Temporary pairs across ordinary call sites -*- lexical-binding: t; -*-
(require 'bytecomp)

;; Multiple blocks keep this helper outside the small MIR inliner. The caller
;; must preserve native-to-native speculation when it changes compilation tier.
(defalias 'neovm-mir-call-step
  (byte-compile (lambda (n) (if (> n 0) (1- n) 0))))
;; An alias emits an ordinary, cell-dispatched Call rather than the dedicated
;; point opcode. Redefinition must continue to invalidate its speculated path.
(fset 'neovm-mir-call-point (symbol-function 'point))

(defalias 'neovm-mir-call-bytecode-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (let ((pair (cons n (1- n))))
           (setq sum (+ sum (+ (car pair) (cdr pair)))))
         (setq n (neovm-mir-call-step n)))
       sum))))

(defalias 'neovm-mir-call-subr-sum
  (byte-compile
   (lambda (n)
     (let ((sum 0))
       (while (> n 0)
         (let ((pair (cons n (1- n))))
           (setq sum (+ sum (+ (car pair) (cdr pair)))))
         (setq sum (+ sum (neovm-mir-call-point)))
         (setq n (1- n)))
       sum))))

(with-temp-buffer
  (insert "abc")
  (goto-char 2)
  (let ((n 100000))
    (dolist (case `((neovm-mir-call-bytecode-sum . ,(* n n))
                    (neovm-mir-call-subr-sum . ,(+ (* n n) (* 2 n)))))
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
                       (cdr case) (- gcs-done before-gcs)))))))

;;; mir-call-bench.el ends here
