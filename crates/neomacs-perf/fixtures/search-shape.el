;;; search-shape.el --- individual bounded search loops -*- lexical-binding: t; -*-
;; Preserve the six-case diagnostic loop, text, bounds and 2,000 + 20 x 100 warmup.
;; Compilation, warmup, collection and result validation are outside sampling.
(require 'bytecomp)
(require 'json)

(defun neomacs-perf-search-shape-loop (n fn pattern start bound)
  (let (answer)
    (dotimes (_ n)
      (goto-char start)
      (setq answer (funcall fn pattern bound t)))
    answer))

(defvar neomacs-perf-search-shape--profile-gate-process nil)
(defvar neomacs-perf-search-shape--profile-gate-response "")

(defun neomacs-perf-search-shape--profile-gate-filter (_process output)
  (setq neomacs-perf-search-shape--profile-gate-response
        (concat neomacs-perf-search-shape--profile-gate-response output)))

(defun neomacs-perf-search-shape--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid search-shape profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p
                     neomacs-perf-search-shape--profile-gate-process)))
      (setq neomacs-perf-search-shape--profile-gate-process
            (make-network-process
             :name "neomacs-perf-search-shape-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-search-shape--profile-gate-filter)))
    neomacs-perf-search-shape--profile-gate-process))

(defun neomacs-perf-search-shape--sampling-command (command)
  (let ((process (neomacs-perf-search-shape--profile-gate-connect)))
    (when process
      (setq neomacs-perf-search-shape--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and
                (not (and
                      (> (length
                          neomacs-perf-search-shape--profile-gate-response)
                         0)
                      (= (aref
                          neomacs-perf-search-shape--profile-gate-response
                          (1- (length
                               neomacs-perf-search-shape--profile-gate-response)))
                         ?\n)))
                (< (float-time) deadline))
          (unless (process-live-p process)
            (error "search-shape profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf-search-shape--profile-gate-response
                       "ack\n")
          (error "search-shape profile gate rejected %s: %S"
                 command
                 neomacs-perf-search-shape--profile-gate-response))))))

(defun neomacs-perf-search-shape--close-profile-gate ()
  (when (processp neomacs-perf-search-shape--profile-gate-process)
    (delete-process neomacs-perf-search-shape--profile-gate-process)
    (setq neomacs-perf-search-shape--profile-gate-process nil)))

(defconst neomacs-perf-search-shape--cases
  '(("search-posix-forward" posix-search-forward "\\(ab\\)" 1 8 4 (2 4 2 4 t))
    ("search-posix-backward" posix-search-backward "\\(ab\\)" 8 1 5 (5 7 5 7 t))
    ("search-regexp-backward" re-search-backward "\\(ab\\)" 8 1 5 (5 7 5 7 t))
    ("search-regexp-forward" re-search-forward "\\(ab\\)" 1 8 4 (2 4 2 4 t))
    ("search-literal-forward" search-forward "ab" 1 8 4 (2 4 t))
    ("search-literal-backward" search-backward "ab" 8 1 5 (5 7 t))))

(defun neomacs-perf-search-shape--match-data ()
  (mapcar (lambda (value) (if (bufferp value)
                              (eq value (current-buffer)) value))
          (match-data t)))

(defun neomacs-perf-search-shape--validate (case answer)
  (unless (and (equal answer (nth 5 case)) (= (point) (nth 5 case))
               (equal (neomacs-perf-search-shape--match-data) (nth 6 case))
               (equal (buffer-string) "éab abc"))
    (error "Wrong search result: %S %S %S" answer (point)
           (neomacs-perf-search-shape--match-data))))

(defun neomacs-perf-search-shape--run ()
  (let* ((scenario (getenv "NEOMACS_PERF_WORKLOAD"))
         (iterations (string-to-number (or (getenv "NEOMACS_PERF_ITERATIONS") "0")))
         (case (assoc scenario neomacs-perf-search-shape--cases))
         (compiled nil) (warmup-validations 0) (elapsed-us 0) (wall-us 0)
         (completed 0) (result-value 0) (final-point 0) (final-match-data "")
         (buffer-contents "") (buffer-bytes 0) (buffer-size 0) (multibyte nil)
         (status "error") (error-message nil) (exit-code 2))
    (condition-case err
        (with-temp-buffer
          (unless (and case (> iterations 0))
            (error "Invalid search scenario or iteration count"))
          (insert "éab abc")
          (let ((case-fold-search nil)
                (args (list (nth 1 case) (nth 2 case) (nth 3 case) (nth 4 case))))
            (byte-compile 'neomacs-perf-search-shape-loop)
            (setq compiled (byte-code-function-p
                            (symbol-function 'neomacs-perf-search-shape-loop)))
            (unless compiled (error "Expected compiled search loop"))
            (set-match-data '(7 9))
            (dotimes (_ 2000)
              (neomacs-perf-search-shape--validate
               case (apply #'neomacs-perf-search-shape-loop 1 args))
              (setq warmup-validations (1+ warmup-validations)))
            (dotimes (_ 20)
              (neomacs-perf-search-shape--validate
               case (apply #'neomacs-perf-search-shape-loop 100 args))
              (setq warmup-validations (1+ warmup-validations)))
            (garbage-collect)
            (neomacs-perf-search-shape--sampling-command "enable")
            (unwind-protect
                (let ((cpu-start (car (current-cpu-time)))
                      (wall-start (float-time)))
                  (setq result-value (apply #'neomacs-perf-search-shape-loop iterations args)
                        wall-us (round (* 1000000 (- (float-time) wall-start)))
                        elapsed-us (- (car (current-cpu-time)) cpu-start)))
              (neomacs-perf-search-shape--sampling-command "disable"))
            (neomacs-perf-search-shape--validate case result-value)
            (setq completed iterations final-point (point)
                  final-match-data (prin1-to-string (neomacs-perf-search-shape--match-data))
                  buffer-contents (buffer-string) buffer-size (buffer-size)
                  buffer-bytes (string-bytes buffer-contents)
                  multibyte enable-multibyte-characters status "ok" exit-code 0)))
      (error (setq error-message (error-message-string err))))
    (neomacs-perf-search-shape--close-profile-gate)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file (getenv "NEOMACS_PERF_RESULT")
        (insert
         (json-serialize
          `((schema_version . 1) (scenario . ,scenario) (status . ,status)
            (iterations . ,iterations) (elapsed_us . ,elapsed-us)
            (elapsed_wall_us . ,wall-us) (completed_operations . ,completed)
            (result_value . ,result-value) (point . ,final-point)
            (match_data . ,final-match-data) (buffer_contents . ,buffer-contents)
            (buffer_size . ,buffer-size) (buffer_bytes . ,buffer-bytes)
            (multibyte . ,(if multibyte t :json-false))
            (warmup_single_calls . 2000) (warmup_batch_calls . 20)
            (warmup_batch_iterations . 100) (warmup_validations . ,warmup-validations)
            (bytecode_compiled . ,(if compiled t :json-false)) (error . ,error-message))
          :false-object :json-false :null-object nil))))
    (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-search-shape--run)
  (run-at-time 0 nil #'neomacs-perf-search-shape--run))
