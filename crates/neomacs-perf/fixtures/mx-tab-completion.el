;;; mx-tab-completion.el --- deterministic M-x TAB workload  -*- lexical-binding: t; -*-

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

(defvar neomacs-perf-mx-tab--elapsed-us 0)
(defvar neomacs-perf-mx-tab--help-calls 0)
(defvar neomacs-perf-mx-tab--completion-visible t)
(defvar neomacs-perf-mx-tab--completion-mode-correct t)
(defvar neomacs-perf-mx-tab--known-commands-present t)
(defvar neomacs-perf-mx-tab--last-completions nil)
(defvar neomacs-perf-mx-tab--candidate-count nil)
(defvar neomacs-perf-mx-tab--current-candidate-count 0)
(defvar neomacs-perf-mx-tab--candidate-count-stable t)

(defun neomacs-perf-mx-tab--candidate-present-p (name candidates)
  (catch 'present
    (while (consp candidates)
      (when (equal name (car candidates))
        (throw 'present t))
      (setq candidates (cdr candidates)))
    nil))

(defun neomacs-perf-mx-tab--capture-completions (original &rest arguments)
  (let ((completions (apply original arguments)))
    ;; A completion style may make nested calls and may use an integer as the
    ;; final cdr for base-size. Retain the largest candidate spine produced by
    ;; this TAB invocation without assuming the result is a proper list.
    (when (and (minibufferp) (consp completions))
      (let ((count (safe-length completions)))
        (when (> count neomacs-perf-mx-tab--current-candidate-count)
          (setq neomacs-perf-mx-tab--current-candidate-count count
                neomacs-perf-mx-tab--last-completions completions))))
    completions))

(defun neomacs-perf-mx-tab--around-completion-help (original &rest arguments)
  (let ((started (car (current-cpu-time))))
    (prog1 (apply original arguments)
      ;; The workload measures the point at which the completion window is
      ;; actually presentable, not merely the end of candidate enumeration.
      (redisplay t)
      (setq neomacs-perf-mx-tab--elapsed-us
            (+ neomacs-perf-mx-tab--elapsed-us
               (- (car (current-cpu-time)) started))
            neomacs-perf-mx-tab--help-calls
            (1+ neomacs-perf-mx-tab--help-calls)
            neomacs-perf-mx-tab--completion-visible
            (and neomacs-perf-mx-tab--completion-visible
                 (get-buffer-window "*Completions*" t))
            neomacs-perf-mx-tab--completion-mode-correct
            (and neomacs-perf-mx-tab--completion-mode-correct
                 (with-current-buffer "*Completions*"
                   (derived-mode-p 'completion-list-mode))))
      ;; Validate the returned completion data after stopping the timer, so
      ;; the oracle's list walks cannot improve or regress the measurement.
      (setq neomacs-perf-mx-tab--candidate-count-stable
            (and neomacs-perf-mx-tab--candidate-count-stable
                 (or (null neomacs-perf-mx-tab--candidate-count)
                     (= neomacs-perf-mx-tab--candidate-count
                        neomacs-perf-mx-tab--current-candidate-count)))
            neomacs-perf-mx-tab--candidate-count
            neomacs-perf-mx-tab--current-candidate-count
            neomacs-perf-mx-tab--known-commands-present
            (and neomacs-perf-mx-tab--known-commands-present
                 (neomacs-perf-mx-tab--candidate-present-p
                  "execute-extended-command"
                  neomacs-perf-mx-tab--last-completions)
                 (neomacs-perf-mx-tab--candidate-present-p
                  "find-file" neomacs-perf-mx-tab--last-completions))))))

(defun neomacs-perf-mx-tab--write-result
    (path status iterations elapsed-us completion-help-calls
          completion-visible completion-mode-correct known-commands-present
          completion-candidate-count candidate-count-stable
          completion-hidden-after-exit minibuffer-depth-restored
          selected-buffer-restored error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1)
        (scenario . "mx-tab-completion")
        (status . ,status)
        (iterations . ,iterations)
        (elapsed_us . ,elapsed-us)
        (completion_help_calls . ,completion-help-calls)
        (completion_visible . ,(neomacs-perf--json-boolean completion-visible))
        (completion_mode_correct
         . ,(neomacs-perf--json-boolean completion-mode-correct))
        (known_commands_present
         . ,(neomacs-perf--json-boolean known-commands-present))
        (completion_candidate_count . ,completion-candidate-count)
        (candidate_count_stable
         . ,(neomacs-perf--json-boolean candidate-count-stable))
        (completion_hidden_after_exit
         . ,(neomacs-perf--json-boolean completion-hidden-after-exit))
        (minibuffer_depth_restored
         . ,(neomacs-perf--json-boolean minibuffer-depth-restored))
        (selected_buffer_restored
         . ,(neomacs-perf--json-boolean selected-buffer-restored))
        (error . ,error-message))
      :false-object :json-false
      :null-object nil))))

(defun neomacs-perf-mx-tab--run ()
  (let* ((result-path
          (neomacs-perf--required-environment "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf--required-environment "SENTINEL"))
         (iterations
          (string-to-number
           (neomacs-perf--required-environment "NEOMACS_PERF_ITERATIONS")))
         (initial-buffer (current-buffer))
         (completed 0)
         (completion-hidden-after-exit t)
         (minibuffer-depth-restored t)
         (selected-buffer-restored t)
         (status "error")
         (error-message nil)
         (exit-code 2))
    (condition-case error-data
        (progn
          (unless (> iterations 0)
            (error "iterations must be positive"))
          (advice-add 'minibuffer-completion-help :around
                      #'neomacs-perf-mx-tab--around-completion-help)
          (advice-add 'completion-all-completions :around
                      #'neomacs-perf-mx-tab--capture-completions)
          (let ((sampling-enabled nil))
            (neomacs-perf--sampling-command "enable")
            (setq sampling-enabled t)
            (unwind-protect
                (dotimes (_ iterations)
                  ;; Select the no-op command after displaying completions.
                  ;; This keeps the benchmark focused on M-x TAB and gives GNU
                  ;; and Neomacs the same normal minibuffer-exit lifecycle.
                  (setq neomacs-perf-mx-tab--last-completions nil
                        neomacs-perf-mx-tab--current-candidate-count 0)
                  (execute-kbd-macro (kbd "M-x TAB i g n o r e RET"))
                  (setq completion-hidden-after-exit
                          (and completion-hidden-after-exit
                               (not (get-buffer-window "*Completions*" t)))
                          minibuffer-depth-restored
                          (and minibuffer-depth-restored
                               (zerop (minibuffer-depth)))
                          selected-buffer-restored
                          (and selected-buffer-restored
                               (eq initial-buffer (current-buffer)))
                          completed (1+ completed)))
              (when sampling-enabled
                (neomacs-perf--sampling-command "disable"))))
          (setq status "ok"
                exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "mx-tab-completion failed: %s" error-message)))
    (advice-remove 'minibuffer-completion-help
                   #'neomacs-perf-mx-tab--around-completion-help)
    (advice-remove 'completion-all-completions
                   #'neomacs-perf-mx-tab--capture-completions)
    (neomacs-perf--close-profile-gate)
    (neomacs-perf-mx-tab--write-result
     result-path status completed neomacs-perf-mx-tab--elapsed-us
     neomacs-perf-mx-tab--help-calls
     neomacs-perf-mx-tab--completion-visible
     neomacs-perf-mx-tab--completion-mode-correct
     neomacs-perf-mx-tab--known-commands-present
     (or neomacs-perf-mx-tab--candidate-count 0)
     neomacs-perf-mx-tab--candidate-count-stable
     completion-hidden-after-exit minibuffer-depth-restored
     selected-buffer-restored error-message)
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-mx-tab--run)
  (run-at-time 0 nil #'neomacs-perf-mx-tab--run))

;;; mx-tab-completion.el ends here
