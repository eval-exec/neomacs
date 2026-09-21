;;; mir-loop-bench.el --- GNU counterpart of jit_bench_mir_loops_against_baseline -*- lexical-binding: t; -*-

;; Run with: taskset -c CORE emacs -Q --batch -l mir-loop-bench.el
;; Measures bytecode, excluding compilation and warmup. Seven samples, five
;; calls per sample; median microseconds per call. No native compilation.
(require 'bytecomp)
(let* ((n 1000000)
       (cases
        `((countdown ,(lambda (n)
                       (let ((acc 0))
                         (while (> n 0) (setq n (1- n))) acc)) 0)
          (sum ,(lambda (n)
                  (let ((acc 0))
                    (while (> n 0) (setq acc (+ acc n) n (1- n))) acc))
               ,(/ (* n (1+ n)) 2))
          (polynomial ,(lambda (n)
                         (let ((acc 0))
                           (while (> n 0)
                             (setq acc (+ acc (+ (+ (* n n) (* n 3)) 7))
                                   n (1- n))) acc))
                      ,(+ (/ (* n (1+ n) (1+ (* 2 n))) 6)
                          (/ (* 3 n (1+ n)) 2) (* 7 n))))))
  (dolist (case cases)
    (let ((fn (byte-compile (nth 1 case))) samples)
      (unless (byte-code-function-p fn) (error "Expected bytecode"))
      (dotimes (round 8)
        (let ((start (float-time)))
          (dotimes (_ 5)
            (unless (= (funcall fn n) (nth 2 case))
              (error "Wrong result for %s" (car case))))
          (when (> round 0)
            (push (/ (* 1e6 (- (float-time) start)) 5) samples))))
      (princ (format "BENCH gnu-loop %s n=%d bytecode_us=%.1f\n"
                     (car case) n (nth 3 (sort samples #'<)))))))
