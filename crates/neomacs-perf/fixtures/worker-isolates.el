;;; worker-isolates.el --- worker isolate performance scenarios -*- lexical-binding: t; -*-

(require 'json)
(require 'bytecomp)

(defun neomacs-perf-worker--required-env (name)
  (or (getenv name) (error "required performance environment %s is absent" name)))

(defun neomacs-perf-worker--work (n)
  (let ((s 0))
    (dotimes (i n) (setq s (logand (+ s (* i i)) #xffffff)))
    s))

(defun neomacs-perf-worker--json (source)
  (let ((v 0))
    (dotimes (_ 20)
      (setq v (+ v (length (json-parse-string source :array-type 'list)))))
    v))

(defun neomacs-perf-worker--time (thunk)
  (let ((started (float-time)))
    (funcall thunk)
    (- (float-time) started)))

(defvar neomacs-perf-worker--gate-process nil)
(defvar neomacs-perf-worker--gate-response "")

(defun neomacs-perf-worker--gate-filter (_process output)
  (setq neomacs-perf-worker--gate-response
        (concat neomacs-perf-worker--gate-response output)))

(defun neomacs-perf-worker--sampling-command (command)
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid performance sampling gate port %S" port-text))
    (when (and port-text
               (not (process-live-p neomacs-perf-worker--gate-process)))
      (setq neomacs-perf-worker--gate-process
            (make-network-process
             :name "neomacs-perf-worker-gate" :family 'ipv4
             :host "127.0.0.1" :service port :coding 'binary :noquery t
             :filter #'neomacs-perf-worker--gate-filter)))
    (when port-text
      (setq neomacs-perf-worker--gate-response "")
      (process-send-string neomacs-perf-worker--gate-process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and (not (string-suffix-p "\n" neomacs-perf-worker--gate-response))
                    (< (float-time) deadline))
          (unless (process-live-p neomacs-perf-worker--gate-process)
            (error "performance sampling gate disconnected"))
          (accept-process-output neomacs-perf-worker--gate-process 0.05))
        (unless (equal neomacs-perf-worker--gate-response "ack\n")
          (error "performance sampling gate rejected %s: %S"
                 command neomacs-perf-worker--gate-response))))))

(defun neomacs-perf-worker--close-sampling-gate ()
  (when (processp neomacs-perf-worker--gate-process)
    (delete-process neomacs-perf-worker--gate-process)
    (setq neomacs-perf-worker--gate-process nil)))

(defun neomacs-perf-worker--with-sampling-gate (thunk)
  (let ((sampling-enabled nil))
    (unwind-protect
        (progn
          (neomacs-perf-worker--sampling-command "enable")
          (setq sampling-enabled t)
          (funcall thunk))
      (when sampling-enabled
        (neomacs-perf-worker--sampling-command "disable")))))

(defun neomacs-perf-worker--run-batch (fn arg tasks)
  "Run FN on ARG as TASKS worker tasks and return their values, in order."
  (let ((handles nil))
    (dotimes (_ tasks)
      (push (neomacs-worker-run fn arg) handles))
    (mapcar #'neomacs-worker-wait (nreverse handles))))

(defun neomacs-perf-worker--check-batch (label values expected)
  "Signal unless every value in VALUES is `equal' to EXPECTED.
Called after the timer has stopped, so the comparison is outside the
measured region, and before any number is recorded."
  (dolist (value values)
    (unless (equal value expected)
      (error "%s: worker result differs from serial result" label))))

(defun neomacs-perf-worker--sample (label fn arg tasks)
  (let* ((expected (funcall fn arg))
         (serial (neomacs-perf-worker--time
                  (lambda () (dotimes (_ tasks) (funcall fn arg)))))
         (first-values nil)
         (parallel (neomacs-perf-worker--time
                    (lambda ()
                      (setq first-values (neomacs-perf-worker--run-batch fn arg tasks)))))
         (warm-values nil)
         (warm (neomacs-perf-worker--time
                (lambda ()
                  (setq warm-values (neomacs-perf-worker--run-batch fn arg tasks))))))
    ;; Validity before timing: both batches are compared with the serial
    ;; value after their timers stopped and before a number is kept.
    (neomacs-perf-worker--check-batch (format "%s first" label) first-values expected)
    (neomacs-perf-worker--check-batch (format "%s warm" label) warm-values expected)
    `((tier . ,label)
      (tasks . ,tasks)
      (serial_seconds . ,serial)
      (parallel_first_seconds . ,parallel)
      (parallel_seconds . ,warm)
      (serial_parallel_ratio . ,(/ serial warm))
      (results_equal . t))))

(defun neomacs-perf-worker--json-source ()
  (concat
   "["
   (mapconcat
    (lambda (i)
      (format "{\"id\":%d,\"name\":\"item-%d\",\"tags\":[\"a\",\"b\",\"c\"],\"v\":%f}"
              i i (/ i 7.0)))
    (number-sequence 1 4000) ",")
   "]"))

(defun neomacs-perf-worker--write-result
    (path status elapsed processors pool-size first-task boot-max boot-mean samples error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1)
        (scenario . ,(neomacs-perf-worker--required-env "NEOMACS_PERF_SCENARIO_ID"))
        (status . ,status)
        (elapsed_us . ,elapsed)
        (num_processors . ,processors)
        (pool_size . ,pool-size)
        (first_task_latency_us . ,first-task)
        (boot_max_us . ,boot-max)
        (boot_mean_us . ,boot-mean)
        (samples . ,(vconcat samples))
        (error . ,error-message))
      :null-object nil))))

(defun neomacs-perf-worker--run ()
  (let* ((scenario (neomacs-perf-worker--required-env "NEOMACS_PERF_SCENARIO_ID"))
         (result-path (neomacs-perf-worker--required-env "NEOMACS_PERF_RESULT"))
         (processors (num-processors))
         (pool-size 0)
         (first-task 0)
         (boot-max 0)
         (boot-mean 0)
         (elapsed 0)
         (samples nil)
         (status "error")
         (error-message nil)
         (started (float-time)))
    (condition-case err
        (progn
          (unless (and (fboundp 'neomacs-worker-run)
                       (fboundp 'neomacs-worker-wait)
                       (fboundp 'neomacs-worker-stats)
                       (fboundp 'neomacs-worker-pool-size))
            (error "editor lacks neomacs-worker-run isolate support"))
          (pcase scenario
            ("worker-isolates-loop"
             (let* ((_first (neomacs-worker-wait
                             (neomacs-worker-run (symbol-function 'identity) '(1))))
                    (pool-size (neomacs-worker-pool-size))
                    (interpreted (symbol-function 'neomacs-perf-worker--work))
                    (tier (if (equal (getenv "NEOVM_JIT") "0")
                              "bytecode-jit-off" "bytecode-jit-on")))
               (byte-compile 'neomacs-perf-worker--work)
               (setq samples
                     (neomacs-perf-worker--with-sampling-gate
                      (lambda ()
                        (append
                             (list
                              (neomacs-perf-worker--sample "interpreted-lambda"
                                interpreted
                                3000000 pool-size)
                              (neomacs-perf-worker--sample "interpreted-lambda"
                                interpreted
                                3000000 (* 4 pool-size))
                              (neomacs-perf-worker--sample tier
                                (symbol-function 'neomacs-perf-worker--work)
                                3000000 pool-size)
                              (neomacs-perf-worker--sample tier
                                (symbol-function 'neomacs-perf-worker--work)
                                3000000 (* 4 pool-size)))))))
               (let ((stats (neomacs-worker-stats)))
                 (setq boot-max (round (* 1000 (plist-get stats :boot-max-ms)))
                       boot-mean (round (* 1000 (plist-get stats :boot-mean-ms)))))))
            ("worker-isolates-json"
             (let* ((fn (symbol-function 'neomacs-perf-worker--json))
                    (_first (neomacs-worker-wait
                             (neomacs-worker-run (symbol-function 'identity) '(1))))
                    (pool-size (neomacs-worker-pool-size))
                    (source (neomacs-perf-worker--json-source)))
               (setq samples
                     (neomacs-perf-worker--with-sampling-gate
                      (lambda ()
                        (list (neomacs-perf-worker--sample "json-parse-string"
                                fn source pool-size)))))
                     (let ((stats (neomacs-worker-stats)))
                       (setq boot-max (round (* 1000 (plist-get stats :boot-max-ms)))
                             boot-mean (round (* 1000 (plist-get stats :boot-mean-ms)))))))
            ("worker-isolates-boot"
             (setq first-task
                   (neomacs-perf-worker--with-sampling-gate
                    (lambda ()
                      (let ((started-task (float-time)))
                        (neomacs-worker-wait
                         (neomacs-worker-run (symbol-function 'identity) '(1)))
                        (round (* 1000000 (- (float-time) started-task)))))))
             (setq pool-size (neomacs-worker-pool-size))
               (let ((stats (neomacs-worker-stats)))
                 (while (< (plist-get stats :ready) pool-size)
                   (accept-process-output nil 0.001)
                   (setq stats (neomacs-worker-stats)))
                 (setq boot-max (round (* 1000 (plist-get stats :boot-max-ms)))
                       boot-mean (round (* 1000 (plist-get stats :boot-mean-ms)))))
               (setq samples
                     (list `((tier . "cold-pool-start") (tasks . 1)
                             (serial_seconds . 0.0)
                             (parallel_first_seconds . ,(/ first-task 1000000.0))
                             (parallel_seconds . ,(/ first-task 1000000.0))
                             (serial_parallel_ratio . 1.0)
                             (results_equal . t)))))
            (_ (error "unknown worker isolate scenario %s" scenario)))
          (setq status "ok"))
      (error
       (setq error-message (error-message-string err))
       (message "worker isolate scenario invalid: %s" error-message)))
    (setq elapsed (round (* 1000000 (- (float-time) started))))
    (let ((stats (and (fboundp 'neomacs-worker-stats)
                      (condition-case nil (neomacs-worker-stats) (error nil)))))
      (setq pool-size (if (fboundp 'neomacs-worker-pool-size)
                          (condition-case nil (neomacs-worker-pool-size) (error 0))
                        pool-size)
            boot-max (or boot-max (round (* 1000 (or (plist-get stats :boot-max-ms) 0))))
            boot-mean (or boot-mean (round (* 1000 (or (plist-get stats :boot-mean-ms) 0))))))
    (neomacs-perf-worker--write-result
     result-path status elapsed processors pool-size first-task boot-max boot-mean
     samples error-message)
    (neomacs-perf-worker--close-sampling-gate)
    (write-region "done\n" nil (neomacs-perf-worker--required-env "SENTINEL") nil 'silent)
    ;; Publish typed scenario failures (including a build without worker
    ;; isolates) as completed runs so the harness can emit INVALID verdicts.
    (kill-emacs 0)))

(if noninteractive
    (neomacs-perf-worker--run)
  (run-at-time 0 nil #'neomacs-perf-worker--run))

;;; worker-isolates.el ends here
