;;; elisp-benchmarks.el --- GNU ELPA elisp-benchmarks driver -*- lexical-binding: t; -*-

;; Runs the UPSTREAM Elisp benchmark suite -- the one GNU uses to evaluate
;; native-comp -- against whichever engine the harness launched.
;;
;; The point of this row is that WE DID NOT WRITE THE WORKLOAD.  Every fixture
;; in this directory was authored here, and an audit of them found that each
;; one either flattered this engine or hid a defect; a third-party suite cannot
;; be shaped, consciously or not, to our strengths.  It is also the right
;; instrument for one specific claim: that our bytecode interpreter beats GNU's
;; while our call seam loses.  The suite's own split between iterative and
;; recursive Fibonacci tests exactly that, and upstream had no such thesis in
;; mind when writing it.
;;
;; This is NOT an editor benchmark and must not be read as one.  Twelve of its
;; eighteen members are arithmetic and list compute.  It is deliberately absent
;; from the standard suite so it cannot enter the board's geometric mean, where
;; it would report a number that predicts nothing a user feels.

(require 'json)

(defun neomacs-perf-elb--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defun neomacs-perf-elb--cpu-us ()
  (car (current-cpu-time)))

(defun neomacs-perf-elb--run ()
  (let* ((result-path (neomacs-perf-elb--required-environment "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf-elb--required-environment "SENTINEL"))
         (package-dir (neomacs-perf-elb--required-environment "NEOMACS_PERF_ELB_DIR"))
         (iterations (string-to-number
                      (neomacs-perf-elb--required-environment "NEOMACS_PERF_ITERATIONS")))
         (report-path (neomacs-perf-elb--required-environment "NEOMACS_PERF_ELB_REPORT"))
         (status "error") (error-message nil) (exit-code 2)
         (elapsed-us 0) (benchmark-count 0) (report "") (completed 0))
    (condition-case error-data
        (progn
          (unless (> iterations 0)
            (error "iterations must be positive"))
          (add-to-list 'load-path package-dir)
          (require 'elisp-benchmarks)
          (setq benchmark-count
                (length (directory-files elb-bench-directory nil "\\.el\\'")))
          (unless (> benchmark-count 0)
            (error "no benchmarks found in %s" elb-bench-directory))
          ;; Byte-compile every benchmark ONCE, outside the timed window: the
          ;; suite recompiles on demand, and compilation is not what this row
          ;; measures.  Neither engine has native-comp, so both take
          ;; `byte-compile-file' here and the comparison stays bytecode to
          ;; bytecode with our JIT working at run time.
          (let ((inhibit-message t))
            (elisp-benchmarks-run nil t 1))
          (let ((captured nil))
            (dotimes (_ iterations)
              (let ((started (neomacs-perf-elb--cpu-us)))
                (cl-letf (((symbol-function 'message)
                           (lambda (format-string &rest arguments)
                             (when format-string
                               (setq captured (apply #'format format-string arguments))))))
                  (elisp-benchmarks-run nil nil 1))
                (setq elapsed-us (+ elapsed-us
                                    (max 1 (- (neomacs-perf-elb--cpu-us) started)))
                      completed (1+ completed))))
            ;; The suite's last message is its results table.  Kept verbatim as
            ;; a side artifact rather than parsed: the per-benchmark split is
            ;; the diagnostic signal, and parsing an upstream presentation
            ;; format would break on their next release for no gain.
            (setq report (or captured "")))
          (setq status "ok" exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "elisp-benchmarks failed: %s" error-message)))
    (with-temp-file report-path (insert report "\n"))
    (with-temp-file result-path
      (insert (json-serialize
               `((schema_version . 1)
                 (scenario . "elisp-benchmarks")
                 (status . ,status)
                 (iterations . ,completed)
                 (elapsed_us . ,elapsed-us)
                 (benchmark_count . ,benchmark-count)
                 (error . ,error-message))
               :false-object :json-false :null-object nil)))
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-elb--run)
  (run-at-time 0 nil #'neomacs-perf-elb--run))

;;; elisp-benchmarks.el ends here
