;;; scrolling.el --- deterministic page-scrolling workload  -*- lexical-binding: t; -*-

(require 'json)

(defvar neomacs-perf--profile-gate-process nil)
(defvar neomacs-perf--profile-gate-response "")

(defun neomacs-perf--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defun neomacs-perf--profile-gate-filter (_process output)
  (setq neomacs-perf--profile-gate-response
        (concat neomacs-perf--profile-gate-response output)))

(defun neomacs-perf--profile-gate-connect ()
  (let* ((port-text (getenv "NEOMACS_PERF_GATE_PORT"))
         (port (and port-text (string-to-number port-text))))
    (when (and port-text (not (> port 0)))
      (error "invalid edit-loop profile gate port %S" port-text))
    (when (and port-text
               (not (process-live-p neomacs-perf--profile-gate-process)))
      (setq neomacs-perf--profile-gate-process
            (make-network-process
             :name "neomacs-perf-gate"
             :family 'ipv4
             :host "127.0.0.1"
             :service port
             :coding 'binary
             :noquery t
             :filter #'neomacs-perf--profile-gate-filter)))
    neomacs-perf--profile-gate-process))

(defun neomacs-perf--sampling-command (command)
  (let ((process (neomacs-perf--profile-gate-connect)))
    (when process
      (setq neomacs-perf--profile-gate-response "")
      (process-send-string process (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and (not (and (> (length neomacs-perf--profile-gate-response) 0)
                              (= (aref neomacs-perf--profile-gate-response
                                       (1- (length neomacs-perf--profile-gate-response)))
                                 ?\n)))
                    (< (float-time) deadline))
          (unless (process-live-p process)
            (error "edit-loop profile gate disconnected during %s" command))
          (accept-process-output process 0.05))
        (unless (equal neomacs-perf--profile-gate-response "ack\n")
          (error "edit-loop profile gate rejected %s: %S"
                 command neomacs-perf--profile-gate-response))))))

(defun neomacs-perf--close-profile-gate ()
  (when (processp neomacs-perf--profile-gate-process)
    (delete-process neomacs-perf--profile-gate-process)
    (setq neomacs-perf--profile-gate-process nil)))

(defun neomacs-perf--json-boolean (value)
  (if value t :json-false))

;;; Buffer construction.

(defconst neomacs-perf-scroll--line-count 2400
  "Buffer size in lines. Large enough that one screenful is a tiny
fraction of the buffer, so scrolling keeps meeting rows the layout
engine has not seen recently instead of re-reading the same dozen.")

(load (expand-file-name "scrolling-content.el" (file-name-directory load-file-name)) nil nil t)

(defun neomacs-perf-scroll--insert-buffer ()
  (neomacs-scroll-content-insert neomacs-perf-scroll--line-count)
  (let ((summary neomacs-scroll-content-summary))
    (with-temp-file (concat (getenv "NEOMACS_PERF_RESULT") ".content.json")
      (insert (json-serialize summary)))))

;;; Timed work.

(defun neomacs-perf-scroll--cpu-us ()
  (car (current-cpu-time)))

(defun neomacs-perf-scroll--at-end-p ()
  (>= (point) (point-max)))

(defun neomacs-perf-scroll--at-start-p ()
  (<= (point) (point-min)))

(defun neomacs-perf-scroll--write-result
    (path status iterations elapsed-us elapsed-wall-us operation-count
          cold-scroll-us warm-scroll-us cold-commands warm-commands
          initial-checksum final-checksum point-restored
          window-start-restored expected-mode actual-mode error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1)
        (scenario . "scrolling")
        (status . ,status)
        (iterations . ,iterations)
        (elapsed_us . ,elapsed-us)
        (elapsed_wall_us . ,elapsed-wall-us)
        (operation_count . ,operation-count)
        (cold_scroll_us . ,cold-scroll-us)
        (warm_scroll_us . ,warm-scroll-us)
        (cold_scroll_commands . ,cold-commands)
        (warm_scroll_commands . ,warm-commands)
        (initial_checksum . ,initial-checksum)
        (final_checksum . ,final-checksum)
        (point_restored . ,(neomacs-perf--json-boolean point-restored))
        (window_start_restored
         . ,(neomacs-perf--json-boolean window-start-restored))
        (expected_major_mode . ,expected-mode)
        (actual_major_mode . ,actual-mode)
        (error . ,error-message))
      :false-object :json-false
      :null-object nil))))

(defun neomacs-perf-scroll--run ()
  (let* ((result-path
          (neomacs-perf--required-environment "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf--required-environment "SENTINEL"))
         (warm-passes
          (string-to-number
           (neomacs-perf--required-environment "NEOMACS_PERF_ITERATIONS")))
         (expected-mode "fundamental-mode")
         (initial-point nil)
         (initial-checksum nil)
         (cold-us 0) (warm-us 0)
         (cold-commands 0) (warm-commands 0)
         (started-wall (float-time))
         (status "error")
         (error-message nil)
         (exit-code 2))
    (condition-case error-data
        (progn
          (unless (> warm-passes 0)
            (error "iterations must be positive"))
          (switch-to-buffer (get-buffer-create "*neomacs-perf-scrolling*"))
          (erase-buffer)
          (neomacs-perf-scroll--insert-buffer)
          (goto-char (point-min))
          (setq initial-point (point)
                initial-checksum
                (secure-hash
                 'sha256
                 (buffer-substring-no-properties (point-min) (point-max))))
          ;; Settle the initial display untimed: the workload times
          ;; SCROLLING, not the first frame.
          (redisplay t)
          (let ((sampling-enabled nil))
            (neomacs-perf--sampling-command "enable")
            (setq sampling-enabled t)
            (unwind-protect
                (progn
                  ;; COLD phase: first display of every line, top to
                  ;; bottom. This is where JIT compilation, face
                  ;; realisation, and first-time layout land, which is
                  ;; exactly why it is timed apart from the warm passes.
                  ;; The workload runs `scroll-up-command'/`scroll-down-
                  ;; command' — what C-v/M-v actually execute — with
                  ;; `scroll-error-top-bottom' bound, so the buffer-edge
                  ;; recovery is GNU's own window.el path, and the loops
                  ;; terminate on position rather than on an error.
                  (let ((scroll-error-top-bottom t)
                        (start (neomacs-perf-scroll--cpu-us))
                        (safety 2000))
                    (while (and (not (neomacs-perf-scroll--at-end-p))
                                (> safety 0))
                      (scroll-up-command nil)
                      (redisplay t)
                      (setq cold-commands (1+ cold-commands)
                            safety (1- safety)))
                    (when (zerop safety)
                      (error "cold pass never reached point-max"))
                    (setq cold-us (- (neomacs-perf-scroll--cpu-us) start)))
                  ;; WARM passes: down and back up over rows the cold pass
                  ;; already laid out. One pass is one iteration.
                  (let ((scroll-error-top-bottom t)
                        (start (neomacs-perf-scroll--cpu-us)))
                    (dotimes (_ warm-passes)
                      (goto-char (point-min))
                      (set-window-start nil (point-min))
                      (while (not (neomacs-perf-scroll--at-end-p))
                        (scroll-up-command nil)
                        (redisplay t)
                        (setq warm-commands (1+ warm-commands)))
                      (while (not (neomacs-perf-scroll--at-start-p))
                        (scroll-down-command nil)
                        (redisplay t)
                        (setq warm-commands (1+ warm-commands))))
                    (setq warm-us (- (neomacs-perf-scroll--cpu-us) start))))
              (when sampling-enabled
                (neomacs-perf--sampling-command "disable"))))
          (setq status "ok"
                exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "scrolling failed: %s" error-message)))
    (let* ((buffer (get-buffer "*neomacs-perf-scrolling*"))
           (live (buffer-live-p buffer))
           (final-checksum
            (and live
                 (with-current-buffer buffer
                   (secure-hash
                    'sha256
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
           (point-restored
            (and live
                 (with-current-buffer buffer
                   (eql (point) initial-point))))
           (window-start-restored
            (and live
                 (get-buffer-window buffer t)
                 (with-current-buffer buffer
                   (= (window-start (get-buffer-window buffer t)) 1))))
           (actual-mode
            (if live
                (with-current-buffer buffer (symbol-name major-mode))
              ""))
           (elapsed-wall-us
            (round (* 1000000 (- (float-time) started-wall)))))
      (neomacs-perf--close-profile-gate)
      (neomacs-perf-scroll--write-result
       result-path status warm-passes
       (+ cold-us warm-us)
       elapsed-wall-us
       (+ cold-commands warm-commands)
       cold-us warm-us cold-commands warm-commands
       (or initial-checksum "") (or final-checksum "")
       point-restored window-start-restored
       expected-mode actual-mode error-message))
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-scroll--run)
  (run-at-time 0 nil #'neomacs-perf-scroll--run))

;;; scrolling.el ends here
