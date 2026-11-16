;;; bench-worker-isolates-tiers.el --- Worker execution tiers benchmark -*- lexical-binding: t; -*-

(defconst s6-worker-bench-iterations 3000000)

(defun s6-worker-bench-loop (n)
  (let ((sum 0))
    (dotimes (i n sum)
      (setq sum (+ sum i)))))

(defun s6-worker-bench-run ()
  (let* ((tier (getenv "NEOMACS_ISOLATE_TIER"))
         (interpreted (equal tier "interpreted"))
         (function (if interpreted
                       (symbol-function 's6-worker-bench-loop)
                     (byte-compile (symbol-function 's6-worker-bench-loop))))
         (cores (num-processors))
         (pool-size (neomacs-worker-pool-size))
         (expected-pool (string-to-number (getenv "NEOMACS_WORKER_THREADS"))))
    (unless (member tier '("interpreted" "bytecode" "jit"))
      (error "Unknown NEOMACS_ISOLATE_TIER: %S" tier))
    (unless (= expected-pool pool-size)
      (error "Expected pool size %d, got %d" expected-pool pool-size))
    ;; Start all owners before timing either form.
    (neomacs-worker-wait (neomacs-worker-run #'identity nil))
    (while (< (plist-get (neomacs-worker-stats) :ready) pool-size)
      (sleep-for 0.01))
    (let* ((serial-start (float-time))
           (serial-result (funcall function s6-worker-bench-iterations))
           (serial-seconds (- (float-time) serial-start))
           (parallel-start (float-time))
           (tasks (make-vector pool-size nil)))
      (dotimes (i pool-size)
        (aset tasks i
              (neomacs-worker-run function s6-worker-bench-iterations)))
      (let ((parallel-results
             (mapcar #'neomacs-worker-wait (append tasks nil)))
            (parallel-seconds (- (float-time) parallel-start)))
        (unless (cl-every (lambda (result) (equal result serial-result))
                          parallel-results)
          (error "Worker results differ from serial result %S: %S"
                 serial-result parallel-results))
        (princ
         (format "| %s | %d | %d | %.3f | %.3f | %.3fx |\n"
                 tier cores pool-size serial-seconds parallel-seconds
                 (/ serial-seconds (max parallel-seconds 0.000001))))))))

(require 'cl-lib)
(s6-worker-bench-run)

;;; bench-worker-isolates-tiers.el ends here
