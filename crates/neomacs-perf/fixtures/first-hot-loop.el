;;; first-hot-loop.el --- first calls through hot loops -*- lexical-binding: t; -*-
(require 'bytecomp)
(require 'json)

;; One operation is a fresh function's first call. Byte compilation and GC are
;; setup; interpreter heat-up, OSR compilation, execution and result collection
;; are timed together. No function is called during preparation.
(defconst neomacs-perf-first-hot-loop--iteration-counts
  '(("first-hot-loop" . 65536)
    ("first-hot-loop-8k" . 8192)
    ("first-hot-loop-16k" . 16384)
    ("first-hot-loop-32k" . 32768)
    ("first-branch-loop-64" . 4096)
    ("first-branch-loop-256" . 4096)))

(defconst neomacs-perf-first-hot-loop--branch-counts
  '(("first-branch-loop-64" . 64)
    ("first-branch-loop-256" . 256)))

(defvar neomacs-perf-first-hot-loop--profile-gate-process nil)
(defvar neomacs-perf-first-hot-loop--profile-gate-response "")

(defun neomacs-perf-first-hot-loop--profile-gate-filter (_process output)
  (setq neomacs-perf-first-hot-loop--profile-gate-response
        (concat neomacs-perf-first-hot-loop--profile-gate-response output)))

(defun neomacs-perf-first-hot-loop--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid first-hot-loop profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p
                     neomacs-perf-first-hot-loop--profile-gate-process)))
      (setq neomacs-perf-first-hot-loop--profile-gate-process
            (make-network-process
             :name "neomacs-perf-first-hot-loop-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-first-hot-loop--profile-gate-filter)))
    neomacs-perf-first-hot-loop--profile-gate-process))

(defun neomacs-perf-first-hot-loop--sampling-command (command)
  ;; Correlate optional OSR diagnostics with the first-call workload. These
  ;; messages are outside its timers and absent from ordinary measurements.
  (when (getenv "NEOMACS_OSR_DEBUG")
    (message "NEOMACS_PERF_FIRST_HOT_PHASE_%s" command))
  (let ((process (neomacs-perf-first-hot-loop--profile-gate-connect)))
    (when process
      (setq neomacs-perf-first-hot-loop--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and
                (not (and
                      (> (length
                          neomacs-perf-first-hot-loop--profile-gate-response)
                         0)
                      (= (aref
                          neomacs-perf-first-hot-loop--profile-gate-response
                          (1- (length
                               neomacs-perf-first-hot-loop--profile-gate-response)))
                         ?\n)))
                (< (float-time) deadline))
          (unless (process-live-p process)
            (error "first-hot-loop profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf-first-hot-loop--profile-gate-response
                       "ack\n")
          (error "first-hot-loop profile gate rejected %s: %S"
                 command
                 neomacs-perf-first-hot-loop--profile-gate-response))))))

(defun neomacs-perf-first-hot-loop--close-profile-gate ()
  (when (processp neomacs-perf-first-hot-loop--profile-gate-process)
    (delete-process neomacs-perf-first-hot-loop--profile-gate-process)
    (setq neomacs-perf-first-hot-loop--profile-gate-process nil)))

(defun neomacs-perf-first-hot-loop--run ()
  (let* ((scenario (getenv "NEOMACS_PERF_WORKLOAD"))
         (inner-iterations
          (or (cdr (assoc scenario neomacs-perf-first-hot-loop--iteration-counts)) 0))
         (branches
          (cdr (assoc scenario neomacs-perf-first-hot-loop--branch-counts)))
         ;; These diamonds remain conditional on the live loop index. Each
         ;; changes outcome during the call; no body is executed in setup.
         (body
          (if branches
              (append
               (mapcar (lambda (cutoff)
                         `(setq sum (+ sum (if (< i ,cutoff) 1 2))))
                       (number-sequence 1 branches))
               '((setq i (1+ i))))
            '((setq sum (+ sum i) i (1+ i)))))
         (iterations (string-to-number (or (getenv "NEOMACS_PERF_ITERATIONS") "0")))
         (functions nil) (results nil) (index 0) (compiled t)
         (prepared 0) (completed 0) (elapsed-us 0) (wall-us 0)
         (status "error") (error-message nil) (exit-code 2))
    (condition-case err
        (progn
          (unless (and (> iterations 0) (> inner-iterations 0))
            (error "Invalid first-hot-loop input"))
          (while (< index iterations)
            ;; Fresh compiler outputs and distinct constants prevent accidental
            ;; reuse of one function's native cache for the entire sample.
            (let ((function
                   (byte-compile
                    `(lambda (n)
                       (let ((held ,index) (i 0) (sum 0))
                         (while (< i n)
                           ,@(copy-tree body))
                         (list i sum held))))))
              (unless (byte-code-function-p function)
                (setq compiled nil)
                (error "First-hot-loop function %d is not bytecode" index))
              (push function functions))
            (setq index (1+ index)))
          (setq functions (nreverse functions) prepared (length functions))
          (garbage-collect)
          (neomacs-perf-first-hot-loop--sampling-command "enable")
          (unwind-protect
              (let ((cpu-start (car (current-cpu-time)))
                    (wall-start (float-time)))
                (dolist (function functions)
                  (push (funcall function inner-iterations)
                        results))
                (setq elapsed-us (- (car (current-cpu-time)) cpu-start)
                      wall-us (round (* 1000000 (- (float-time) wall-start)))))
            (neomacs-perf-first-hot-loop--sampling-command "disable"))
          ;; Keep every result for independent host validation after timing.
          (setq results (nreverse results) completed (length results)
                status "ok" exit-code 0))
      (error (setq error-message (error-message-string err))))
    (neomacs-perf-first-hot-loop--close-profile-gate)
    (with-temp-file (getenv "NEOMACS_PERF_RESULT")
      (insert
       (json-serialize
        `((schema_version . 1) (scenario . ,scenario) (status . ,status)
          (iterations . ,iterations)
          (inner_iterations . ,inner-iterations)
          ,@(when branches `((branches_per_iteration . ,branches)))
          (prepared_functions . ,prepared)
          (bytecode_compiled . ,(if compiled t :json-false))
          (completed_operations . ,completed)
          (results . ,(vconcat (mapcar #'vconcat results)))
          (elapsed_us . ,elapsed-us) (elapsed_wall_us . ,wall-us)
          (error . ,error-message))
        :false-object :json-false :null-object nil)))
    (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-first-hot-loop--run)
  (run-at-time 0 nil #'neomacs-perf-first-hot-loop--run))

;;; first-hot-loop.el ends here
