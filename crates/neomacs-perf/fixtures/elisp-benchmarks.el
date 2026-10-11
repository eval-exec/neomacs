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
(require 'cl-lib)

(defvar neomacs-perf-elb--profile-gate-process nil)
(defvar neomacs-perf-elb--profile-gate-response "")

(defun neomacs-perf-elb--profile-gate-filter (_process output)
  (setq neomacs-perf-elb--profile-gate-response
        (concat neomacs-perf-elb--profile-gate-response output)))

(defun neomacs-perf-elb--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid elisp-benchmarks profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p neomacs-perf-elb--profile-gate-process)))
      (setq neomacs-perf-elb--profile-gate-process
            (make-network-process
             :name "neomacs-perf-elb-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-elb--profile-gate-filter)))
    neomacs-perf-elb--profile-gate-process))

(defun neomacs-perf-elb--sampling-command (command)
  (let ((process (neomacs-perf-elb--profile-gate-connect)))
    (when process
      (setq neomacs-perf-elb--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and (not (and (> (length neomacs-perf-elb--profile-gate-response) 0)
                              (= (aref neomacs-perf-elb--profile-gate-response
                                       (1- (length neomacs-perf-elb--profile-gate-response)))
                                 ?\n)))
                    (< (float-time) deadline))
          (unless (process-live-p process)
            (error "elisp-benchmarks profile gate disconnected during %s" command))
          ;; Suppress timers and unrelated processes during the perf
          ;; handshake; their callbacks are outside the measured workload.
          (accept-process-output process 0.05 nil 1))
        (unless (equal neomacs-perf-elb--profile-gate-response "ack\n")
          (error "elisp-benchmarks profile gate rejected %s: %S"
                 command neomacs-perf-elb--profile-gate-response))))))

(defun neomacs-perf-elb--close-profile-gate ()
  (when (processp neomacs-perf-elb--profile-gate-process)
    (delete-process neomacs-perf-elb--profile-gate-process)
    (setq neomacs-perf-elb--profile-gate-process nil)))


(defun neomacs-perf-elb--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defun neomacs-perf-elb--cpu-us ()
  (car (current-cpu-time)))

(defun neomacs-perf-elb--checked-message (format-string &rest arguments)
  "Format upstream output, rejecting its demoted load and workload errors."
  (when format-string
    (let ((text (apply #'format format-string arguments)))
      ;; The pinned upstream suite catches these errors and calls `message'.
      ;; Reject here, inside that handler, before a later table hides failure.
      (when (or (string-prefix-p "Error loading:" text)
                (string-prefix-p "Error running:" text))
        (error "elisp-benchmarks upstream failure: %s" text))
      text)))

(defun neomacs-perf-elb--run ()
  (let* ((result-path (neomacs-perf-elb--required-environment "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf-elb--required-environment "SENTINEL"))
         (package-dir (neomacs-perf-elb--required-environment "NEOMACS_PERF_ELB_DIR"))
         (iterations (string-to-number
                      (neomacs-perf-elb--required-environment "NEOMACS_PERF_ITERATIONS")))
         (report-path (neomacs-perf-elb--required-environment "NEOMACS_PERF_ELB_REPORT"))
         (status "error") (error-message nil) (exit-code 2)
         (elapsed-us 0) (wall-us 0) (benchmark-count 0) (report "") (completed 0)
         (gc-start-count 0) (gc-end-count 0) (gc-start-us 0) (gc-end-us 0))
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
            (cl-letf (((symbol-function 'message)
                       #'neomacs-perf-elb--checked-message))
              (elisp-benchmarks-run nil t 1)))
          ;; One acknowledged interval encloses every requested suite run.
          ;; Preparation above, gate waits, report serialization and process
          ;; startup are outside the workload CPU/wall timers. GC boundaries
          ;; enclose both handshakes, like the editor fixtures, so their work
          ;; cannot disappear from the count/time deltas.
          (setq gc-start-count gcs-done
                gc-end-count gc-start-count
                gc-start-us (round (* 1000000 gc-elapsed))
                gc-end-us gc-start-us)
          (let ((captured nil))
            (cl-letf (((symbol-function 'message)
                       (lambda (format-string &rest arguments)
                         (when format-string
                           (setq captured
                                 (apply #'neomacs-perf-elb--checked-message
                                        format-string arguments))))))
              (unwind-protect
                  (progn
                    ;; Cleanup also covers an enable request whose ACK fails
                    ;; after the controller may already have enabled counters.
                    (neomacs-perf-elb--sampling-command "enable")
                    (let ((cpu-start (neomacs-perf-elb--cpu-us))
                          (wall-start (float-time)))
                      (dotimes (_ iterations)
                        (elisp-benchmarks-run nil nil 1)
                        (setq completed (1+ completed)))
                      (setq elapsed-us (- (neomacs-perf-elb--cpu-us) cpu-start)
                            wall-us (round (* 1000000 (- (float-time) wall-start))))))
                (unwind-protect
                    (neomacs-perf-elb--sampling-command "disable")
                  (setq gc-end-count gcs-done
                        gc-end-us (round (* 1000000 gc-elapsed))))))
            ;; The suite's last message is its results table. Kept verbatim
            ;; as a side artifact rather than parsed: the per-benchmark split
            ;; is diagnostic, not a separate hardware-counter interval.
            (setq report (or captured "")))
          (setq status "ok" exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "elisp-benchmarks failed: %s" error-message)))
    (neomacs-perf-elb--close-profile-gate)
    (with-temp-file report-path (insert report "\n"))
    (with-temp-file result-path
      (insert (json-serialize
               `((schema_version . 1)
                 (scenario . "elisp-benchmarks")
                 (status . ,status)
                 (iterations . ,completed)
                 (elapsed_us . ,elapsed-us)
                 (elapsed_wall_us . ,wall-us)
                 (gcs_done_start . ,gc-start-count)
                 (gcs_done_end . ,gc-end-count)
                 (gcs_done_delta . ,(- gc-end-count gc-start-count))
                 (gc_elapsed_us_start . ,gc-start-us)
                 (gc_elapsed_us_end . ,gc-end-us)
                 (gc_elapsed_us_delta . ,(- gc-end-us gc-start-us))
                 (benchmark_count . ,benchmark-count)
                 (error . ,error-message))
               :false-object :json-false :null-object nil)))
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-elb--run)
  (run-at-time 0 nil #'neomacs-perf-elb--run))

;;; elisp-benchmarks.el ends here
