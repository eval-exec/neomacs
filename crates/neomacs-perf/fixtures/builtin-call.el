;;; builtin-call.el --- warmed builtin calls from bytecode -*- lexical-binding: t; -*-
;; Each operation includes the bytecompiled loop's dispatch and bookkeeping.
;; Byte compilation, 40 x 2,000 warmup calls and result checks are outside sampling.
(require 'bytecomp)
(require 'json)

;; Aliases keep the compiler from replacing these calls with dedicated opcodes.
(fset 'neomacs-perf-builtin-call-zero (symbol-function 'point))
(fset 'neomacs-perf-builtin-call-one (symbol-function 'string-bytes))
(fset 'neomacs-perf-builtin-call-two (symbol-function 'string-lessp))
(fset 'neomacs-perf-builtin-call-three (symbol-function 'get-text-property))
(fset 'neomacs-perf-builtin-call-multibyte (symbol-function 'multibyte-string-p))
(fset 'neomacs-perf-builtin-call-character (symbol-function 'char-or-string-p))
(fset 'neomacs-perf-builtin-call-vector-control (symbol-function 'max-char))
(defun neomacs-perf-builtin-call-loop-zero (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-zero))) answer))
(defun neomacs-perf-builtin-call-loop-one (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-one "abc"))) answer))
(defun neomacs-perf-builtin-call-loop-two (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-two "abc" "def"))) answer))
(defun neomacs-perf-builtin-call-loop-three (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-three 0 'face "abc"))) answer))
(defun neomacs-perf-builtin-call-loop-multibyte (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-multibyte "Aé中"))) answer))
(defun neomacs-perf-builtin-call-loop-character (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-character #x3fffff))) answer))
(defun neomacs-perf-builtin-call-loop-vector-control (n)
  (let ((answer nil))
    (dotimes (_ n) (setq answer (neomacs-perf-builtin-call-vector-control t))) answer))

(defvar neomacs-perf-builtin-call--profile-gate-process nil)
(defvar neomacs-perf-builtin-call--profile-gate-response "")

(defun neomacs-perf-builtin-call--profile-gate-filter (_process output)
  (setq neomacs-perf-builtin-call--profile-gate-response
        (concat neomacs-perf-builtin-call--profile-gate-response output)))

(defun neomacs-perf-builtin-call--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid builtin-call profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p
                     neomacs-perf-builtin-call--profile-gate-process)))
      (setq neomacs-perf-builtin-call--profile-gate-process
            (make-network-process
             :name "neomacs-perf-builtin-call-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-builtin-call--profile-gate-filter)))
    neomacs-perf-builtin-call--profile-gate-process))

(defun neomacs-perf-builtin-call--sampling-command (command)
  (let ((process (neomacs-perf-builtin-call--profile-gate-connect)))
    (when process
      (setq neomacs-perf-builtin-call--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and
                (not (and
                      (> (length
                          neomacs-perf-builtin-call--profile-gate-response)
                         0)
                      (= (aref
                          neomacs-perf-builtin-call--profile-gate-response
                          (1- (length
                               neomacs-perf-builtin-call--profile-gate-response)))
                         ?\n)))
                (< (float-time) deadline))
          (unless (process-live-p process)
            (error "builtin-call profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf-builtin-call--profile-gate-response
                       "ack\n")
          (error "builtin-call profile gate rejected %s: %S"
                 command
                 neomacs-perf-builtin-call--profile-gate-response))))))

(defun neomacs-perf-builtin-call--close-profile-gate ()
  (when (processp neomacs-perf-builtin-call--profile-gate-process)
    (delete-process neomacs-perf-builtin-call--profile-gate-process)
    (setq neomacs-perf-builtin-call--profile-gate-process nil)))

(defconst neomacs-perf-builtin-call--cases
  '(("builtin-call-point" neomacs-perf-builtin-call-loop-zero neomacs-perf-builtin-call-zero 1)
    ("builtin-call-string-bytes" neomacs-perf-builtin-call-loop-one neomacs-perf-builtin-call-one 3)
    ("builtin-call-string-lessp" neomacs-perf-builtin-call-loop-two neomacs-perf-builtin-call-two t)
    ("builtin-call-get-text-property" neomacs-perf-builtin-call-loop-three neomacs-perf-builtin-call-three nil)
    ("builtin-call-multibyte-string-p" neomacs-perf-builtin-call-loop-multibyte neomacs-perf-builtin-call-multibyte t)
    ("builtin-call-char-or-string-p" neomacs-perf-builtin-call-loop-character neomacs-perf-builtin-call-character t)
    ("builtin-call-max-char" neomacs-perf-builtin-call-loop-vector-control neomacs-perf-builtin-call-vector-control #x10ffff)))

(defun neomacs-perf-builtin-call--run ()
  (let* ((scenario (getenv "NEOMACS_PERF_WORKLOAD"))
         (iterations (string-to-number (or (getenv "NEOMACS_PERF_ITERATIONS") "0")))
         (case (assoc scenario neomacs-perf-builtin-call--cases))
         (function (nth 1 case)) (alias (nth 2 case)) (expected (nth 3 case))
         (compiled nil) (alias-retained nil) (warmup nil)
         (elapsed-us 0) (wall-us 0) (completed 0) (result-value "")
         (status "error") (error-message nil) (exit-code 2))
    (condition-case err
        (with-temp-buffer
          (unless (and case (> iterations 0))
            (error "Invalid builtin call scenario or iteration count"))
          (byte-compile function)
          (setq compiled (byte-code-function-p (symbol-function function))
                alias-retained
                (and compiled
                     (memq alias (append (aref (symbol-function function) 2) nil))
                     t))
          (unless (and compiled alias-retained)
            (error "Expected bytecode retaining call alias %S" alias))
          (dotimes (_ 40)
            (let ((value (funcall function 2000)))
              (unless (equal value expected)
                (error "Wrong warmup result for %s: %S" scenario value))
              (push (prin1-to-string value) warmup)))
          (garbage-collect)
          (neomacs-perf-builtin-call--sampling-command "enable")
          (let (value)
            (unwind-protect
                (let ((cpu-start (car (current-cpu-time)))
                      (wall-start (float-time)))
                  (setq value (funcall function iterations)
                        elapsed-us (- (car (current-cpu-time)) cpu-start)
                        wall-us (round (* 1000000 (- (float-time) wall-start)))))
              (neomacs-perf-builtin-call--sampling-command "disable"))
            (setq result-value (prin1-to-string value) completed iterations)
            (unless (equal value expected)
              (error "Wrong measured result for %s: %S" scenario value)))
          (setq status "ok" exit-code 0))
      (error (setq error-message (error-message-string err))))
    (neomacs-perf-builtin-call--close-profile-gate)
    (with-temp-file (getenv "NEOMACS_PERF_RESULT")
      (insert
       (json-serialize
        `((schema_version . 1) (scenario . ,scenario) (status . ,status)
          (iterations . ,iterations) (elapsed_us . ,elapsed-us)
          (elapsed_wall_us . ,wall-us) (completed_operations . ,completed)
          (result_value . ,result-value) (call_alias . ,(symbol-name alias))
          (warmup_iterations . 2000) (warmup_results . ,(vconcat (nreverse warmup)))
          (bytecode_compiled . ,(if compiled t :json-false))
          (alias_retained . ,(if alias-retained t :json-false)) (error . ,error-message))
        :false-object :json-false :null-object nil)))
    (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-builtin-call--run)
  (run-at-time 0 nil #'neomacs-perf-builtin-call--run))

;;; builtin-call.el ends here
