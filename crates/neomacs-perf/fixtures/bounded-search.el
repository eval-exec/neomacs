;;; bounded-search.el --- bounded search after distant edits -*- lexical-binding: t; -*-
(require 'bytecomp)
(require 'json)

(defvar neomacs-perf-bounded-search--profile-gate-process nil)
(defvar neomacs-perf-bounded-search--profile-gate-response "")

(defun neomacs-perf-bounded-search--profile-gate-filter (_process output)
  (setq neomacs-perf-bounded-search--profile-gate-response
        (concat neomacs-perf-bounded-search--profile-gate-response output)))

(defun neomacs-perf-bounded-search--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid bounded-search profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p
                     neomacs-perf-bounded-search--profile-gate-process)))
      (setq neomacs-perf-bounded-search--profile-gate-process
            (make-network-process
             :name "neomacs-perf-bounded-search-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf-bounded-search--profile-gate-filter)))
    neomacs-perf-bounded-search--profile-gate-process))

(defun neomacs-perf-bounded-search--sampling-command (command)
  (let ((process (neomacs-perf-bounded-search--profile-gate-connect)))
    (when process
      (setq neomacs-perf-bounded-search--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and
                (not (and
                      (> (length
                          neomacs-perf-bounded-search--profile-gate-response)
                         0)
                      (= (aref
                          neomacs-perf-bounded-search--profile-gate-response
                          (1- (length
                               neomacs-perf-bounded-search--profile-gate-response)))
                         ?\n)))
                (< (float-time) deadline))
          (unless (process-live-p process)
            (error "bounded-search profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf-bounded-search--profile-gate-response
                       "ack\n")
          (error "bounded-search profile gate rejected %s: %S"
                 command
                 neomacs-perf-bounded-search--profile-gate-response))))))

(defun neomacs-perf-bounded-search--close-profile-gate ()
  (when (processp neomacs-perf-bounded-search--profile-gate-process)
    (delete-process neomacs-perf-bounded-search--profile-gate-process)
    (setq neomacs-perf-bounded-search--profile-gate-process nil)))


(defun neomacs-perf-bounded-search--loop (iterations edit-at edit search)
  (let ((completed 0) (failures 0) (case-fold-search nil))
    (dotimes (_ iterations)
      (when edit
        (goto-char edit-at)
        (insert "a")
        (delete-char -1))
      (goto-char 1)
      (when search
        (if (re-search-forward "z" 9 t)
            (error "Unexpected bounded search match")
          (setq failures (1+ failures))))
      (unless (= (point) 1) (error "Bounded failure moved point"))
      (setq completed (1+ completed)))
    (list completed failures)))

(byte-compile 'neomacs-perf-bounded-search--loop)

(defun neomacs-perf-bounded-search--run ()
  (let* ((scenario (getenv "NEOMACS_PERF_WORKLOAD"))
         (iterations (string-to-number (or (getenv "NEOMACS_PERF_ITERATIONS") "0")))
         (size (string-to-number (or (getenv "NEOMACS_PERF_BUFFER_SIZE") "0")))
         (edit (equal (getenv "NEOMACS_PERF_EDIT") "1"))
         (search (equal (getenv "NEOMACS_PERF_SEARCH") "1"))
         (compiled (byte-code-function-p
                    (symbol-function 'neomacs-perf-bounded-search--loop)))
         (elapsed-us 0) (wall-us 0) (completed 0) (failures 0)
         (actual-size 0) (actual-bytes 0) (actual-multibyte nil)
         (final-point 0) (data []) (initial "") (final "")
         (status "error") (error-message nil) (exit-code 2))
    (condition-case err
        (with-temp-buffer
          (unless (and (> iterations 0) (>= size 9) compiled)
            (error "Invalid bounded search input or uncompiled loop"))
          (set-buffer-multibyte t)
          (insert "aaaaaaaa中" (make-string (- size 9) ?a))
          (let ((edit-at (1+ (/ size 2))))
            (neomacs-perf-bounded-search--loop 100 edit-at edit search)
            (setq initial (secure-hash 'sha256 (current-buffer)))
            (garbage-collect)
            (set-match-data '(7 9))
            (neomacs-perf-bounded-search--sampling-command "enable")
            (unwind-protect
                (let* ((cpu-start (car (current-cpu-time)))
                       (wall-start (float-time))
                       (result (neomacs-perf-bounded-search--loop
                                iterations edit-at edit search)))
                  (setq elapsed-us (- (car (current-cpu-time)) cpu-start)
                        wall-us (round (* 1000000 (- (float-time) wall-start)))
                        completed (car result) failures (cadr result)
                        final-point (point) data (vconcat (match-data t))))
              (neomacs-perf-bounded-search--sampling-command "disable")))
          (setq actual-size (buffer-size)
                actual-bytes (1- (position-bytes (point-max)))
                actual-multibyte enable-multibyte-characters
                final (secure-hash 'sha256 (current-buffer))
                status "ok" exit-code 0))
      (error (setq error-message (error-message-string err))))
    (neomacs-perf-bounded-search--close-profile-gate)
    (with-temp-file (getenv "NEOMACS_PERF_RESULT")
      (insert
       (json-serialize
        `((schema_version . 1) (scenario . ,scenario) (status . ,status)
          (iterations . ,iterations) (elapsed_us . ,elapsed-us)
          (elapsed_wall_us . ,wall-us) (buffer_size . ,actual-size)
          (buffer_bytes . ,actual-bytes) (multibyte . ,(if actual-multibyte t :json-false))
          (edit . ,(if edit t :json-false)) (search . ,(if search t :json-false))
          (completed_operations . ,completed) (failed_searches . ,failures)
          (point . ,final-point) (match_data . ,data)
          (initial_checksum . ,initial) (final_checksum . ,final)
          (bytecode_compiled . ,(if compiled t :json-false)) (error . ,error-message))
        :false-object :json-false :null-object nil)))
    (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-bounded-search--run)
  (run-at-time 0 nil #'neomacs-perf-bounded-search--run))

;;; bounded-search.el ends here
