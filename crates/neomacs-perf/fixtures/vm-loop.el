;;; vm-loop.el --- rarely called hot loops -*- lexical-binding: t; -*-
(require 'bytecomp)
(require 'json)

(defvar neomacs-perf-vm-loop--special 17)
(defvar neomacs-perf-vm-loop--alias)

(defun neomacs-perf-vm-loop--lexical (iterations)
  (let ((held 7) (i 0) (sum 0))
    (while (< i iterations)
      (setq sum (+ sum i) i (1+ i)))
    (list i sum held)))

(defun neomacs-perf-vm-loop--dynamic (iterations)
  (let ((neomacs-perf-vm-loop--special 7) (i 0) (sum 0))
    (while (< i iterations)
      (setq sum (+ sum i) i (1+ i)))
    (list i sum neomacs-perf-vm-loop--special)))

(defun neomacs-perf-vm-loop--dynamic-read (iterations)
  (let ((neomacs-perf-vm-loop--special 7) (i 0) (sum 0))
    (while (< i iterations)
      (setq sum (+ sum neomacs-perf-vm-loop--special) i (1+ i)))
    (list i sum neomacs-perf-vm-loop--special)))

(defun neomacs-perf-vm-loop--dynamic-rebind (iterations)
  (let ((neomacs-perf-vm-loop--special 7) (i 0) (sum 0))
    (while (< i iterations)
      (let ((neomacs-perf-vm-loop--special i))
        (setq sum (+ sum neomacs-perf-vm-loop--special)))
      (setq i (1+ i)))
    (list i sum neomacs-perf-vm-loop--special)))

(defun neomacs-perf-vm-loop--alias-read (iterations)
  (let ((neomacs-perf-vm-loop--special 7) (i 0) (sum 0))
    (while (< i iterations)
      (setq sum (+ sum neomacs-perf-vm-loop--alias) i (1+ i)))
    (list i sum neomacs-perf-vm-loop--special)))

(byte-compile 'neomacs-perf-vm-loop--lexical)
(byte-compile 'neomacs-perf-vm-loop--dynamic)
(byte-compile 'neomacs-perf-vm-loop--dynamic-read)
(byte-compile 'neomacs-perf-vm-loop--dynamic-rebind)
(byte-compile 'neomacs-perf-vm-loop--alias-read)
;; Preserve the alias operand in bytecode; resolve it dynamically at runtime.
(defvaralias 'neomacs-perf-vm-loop--alias 'neomacs-perf-vm-loop--special)

(defvar neomacs-perf-vm-loop--profile-gate-process nil)
(defvar neomacs-perf-vm-loop--profile-gate-response "")

(defun neomacs-perf-vm-loop--profile-gate-filter (_process output)
  (setq neomacs-perf-vm-loop--profile-gate-response
        (concat neomacs-perf-vm-loop--profile-gate-response output)))

(defun neomacs-perf-vm-loop--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid vm-loop profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p
                     neomacs-perf-vm-loop--profile-gate-process)))
      (setq neomacs-perf-vm-loop--profile-gate-process
            (make-network-process
             :name "neomacs-perf-vm-loop-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-vm-loop--profile-gate-filter)))
    neomacs-perf-vm-loop--profile-gate-process))

(defun neomacs-perf-vm-loop--sampling-command (command)
  (let ((process (neomacs-perf-vm-loop--profile-gate-connect)))
    (when process
      (setq neomacs-perf-vm-loop--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and
                (not (and
                      (> (length
                          neomacs-perf-vm-loop--profile-gate-response)
                         0)
                      (= (aref
                          neomacs-perf-vm-loop--profile-gate-response
                          (1- (length
                               neomacs-perf-vm-loop--profile-gate-response)))
                         ?\n)))
                (< (float-time) deadline))
          (unless (process-live-p process)
            (error "vm-loop profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf-vm-loop--profile-gate-response
                       "ack\n")
          (error "vm-loop profile gate rejected %s: %S"
                 command
                 neomacs-perf-vm-loop--profile-gate-response))))))

(defun neomacs-perf-vm-loop--close-profile-gate ()
  (when (processp neomacs-perf-vm-loop--profile-gate-process)
    (delete-process neomacs-perf-vm-loop--profile-gate-process)
    (setq neomacs-perf-vm-loop--profile-gate-process nil)))

(defun neomacs-perf-vm-loop--run ()
  (let* ((scenario (getenv "NEOMACS_PERF_WORKLOAD"))
         (iterations (string-to-number (or (getenv "NEOMACS_PERF_ITERATIONS") "0")))
         (dynamic (member scenario '("dynamic-binding-loop"
                                     "dynamic-variable-read-loop"
                                     "dynamic-rebinding-loop"
                                     "dynamic-alias-read-loop" "buffer-local-read-loop")))
         (function (cond ((member scenario '("dynamic-variable-read-loop" "buffer-local-read-loop"))
                          #'neomacs-perf-vm-loop--dynamic-read)
                         ((equal scenario "dynamic-alias-read-loop")
                          #'neomacs-perf-vm-loop--alias-read)
                         ((equal scenario "dynamic-rebinding-loop")
                          #'neomacs-perf-vm-loop--dynamic-rebind)
                         (dynamic #'neomacs-perf-vm-loop--dynamic)
                         (t #'neomacs-perf-vm-loop--lexical)))
         (compiled (byte-code-function-p (symbol-function function)))
         (_local-setup (when (equal scenario "buffer-local-read-loop")
                         (make-local-variable 'neomacs-perf-vm-loop--special)))
         (buffer-local (local-variable-p 'neomacs-perf-vm-loop--special))
         (outer-before neomacs-perf-vm-loop--special)
         (outer-after 0) (warmup-outer 0) (warmup [0 0 0])
         (global-after 0) (warmup-global 0)
         (elapsed-us 0) (wall-us 0) (completed 0) (sum 0) (held 0)
         (status "error") (error-message nil) (exit-code 2))
    (condition-case err
        (progn
          (unless (and (> iterations 0) compiled
                       (member scenario '("lexical-loop" "dynamic-binding-loop"
                                          "dynamic-variable-read-loop" "dynamic-rebinding-loop"
                                          "dynamic-alias-read-loop" "buffer-local-read-loop")))
            (error "Invalid VM loop input or uncompiled loop"))
          ;; One short call warms startup paths without forcing native entry.
          ;; The timed call must get hot from its own backward branches.
          (setq warmup (vconcat (funcall function 100))
                warmup-outer neomacs-perf-vm-loop--special
                warmup-global (default-value 'neomacs-perf-vm-loop--special))
          (garbage-collect)
          (neomacs-perf-vm-loop--sampling-command "enable")
          (unwind-protect
              (let* ((cpu-start (car (current-cpu-time)))
                     (wall-start (float-time))
                     (result (funcall function iterations)))
                (setq elapsed-us (- (car (current-cpu-time)) cpu-start)
                      wall-us (round (* 1000000 (- (float-time) wall-start)))
                      completed (car result) sum (cadr result) held (caddr result)))
            (neomacs-perf-vm-loop--sampling-command "disable"))
          (setq outer-after neomacs-perf-vm-loop--special
                global-after (default-value 'neomacs-perf-vm-loop--special)
                status "ok" exit-code 0))
      (error (setq error-message (error-message-string err))))
    (neomacs-perf-vm-loop--close-profile-gate)
    (with-temp-file (getenv "NEOMACS_PERF_RESULT")
      (insert
       (json-serialize
        `((schema_version . 1) (scenario . ,scenario) (status . ,status)
          (iterations . ,iterations) (elapsed_us . ,elapsed-us)
          (elapsed_wall_us . ,wall-us) (completed_operations . ,completed)
          (result_sum . ,sum) (held_value . ,held)
          (outer_value_before . ,outer-before) (outer_value_after . ,outer-after)
          (warmup_result . ,warmup) (warmup_outer_value . ,warmup-outer)
          (global_value_after . ,global-after) (warmup_global_value . ,warmup-global)
          (buffer_local . ,(if buffer-local t :json-false))
          (dynamic_binding . ,(if dynamic t :json-false))
          (bytecode_compiled . ,(if compiled t :json-false)) (error . ,error-message))
        :false-object :json-false :null-object nil)))
    (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-vm-loop--run)
  (run-at-time 0 nil #'neomacs-perf-vm-loop--run))

;;; vm-loop.el ends here
