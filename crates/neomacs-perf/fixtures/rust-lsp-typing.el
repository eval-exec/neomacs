;;; rust-lsp-typing.el --- deterministic heavy-edit workload  -*- lexical-binding: t; -*-

(require 'json)
(require 'lsp-mode)
(require 'rust-ts-mode)
(require 'seq)

(defun neomacs-perf--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defvar neomacs-perf--profile-gate-process nil)
(defvar neomacs-perf--profile-gate-response "")

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

(defun neomacs-perf--write-result
    (path status iterations elapsed-us major-mode-name parser-language
          text-unchanged point-unchanged overlay-count lsp-diagnostic-count
          error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1)
        (scenario . "rust-lsp-typing")
        (status . ,status)
        (iterations . ,iterations)
        (elapsed_us . ,elapsed-us)
        (major_mode . ,major-mode-name)
        (lsp_mode_loaded . ,(neomacs-perf--json-boolean (featurep 'lsp-mode)))
        (treesit_parser_language . ,parser-language)
        (text_unchanged . ,(neomacs-perf--json-boolean text-unchanged))
        (point_unchanged . ,(neomacs-perf--json-boolean point-unchanged))
        (overlay_count . ,overlay-count)
        (lsp_diagnostic_count . ,lsp-diagnostic-count)
        (error . ,error-message))
      :false-object :json-false
      :null-object nil))))

(defun neomacs-perf--lsp-position (position)
  (save-excursion
    (goto-char (point-min))
    (forward-line (neomacs-perf--json-get position "line"))
    (forward-char (min (neomacs-perf--json-get position "character")
                       (- (line-end-position) (point))))
    (point)))

(defun neomacs-perf--diagnostic-face (severity)
  (pcase severity
    (1 'error)
    (2 'warning)
    (_ 'shadow)))

(defun neomacs-perf--json-get (object key)
  (if (hash-table-p object)
      (gethash key object)
    (plist-get object (intern (concat ":" key)))))

(defun neomacs-perf--apply-diagnostic-replay (workspace replay-json)
  (let* ((message (json-parse-string replay-json
                                     :object-type 'hash-table
                                     :array-type 'list
                                     :null-object nil
                                     :false-object nil))
         (params (neomacs-perf--json-get message "params")))
    ;; Exercise LSP Mode's real workspace diagnostics update before applying
    ;; the captured presentation data to overlays in this visible buffer.
    (lsp--on-diagnostics workspace params)
    (remove-overlays (point-min) (point-max) 'neomacs-perf-lsp t)
    ;; Render only diagnostics that LSP Mode actually accepted into its
    ;; workspace state. A broken/no-op handler therefore produces no overlays
    ;; and fails both observable correctness gates.
    (dolist (diagnostic
             (seq-mapcat #'identity
                         (hash-table-values
                          (lsp--workspace-diagnostics workspace))))
      (let* ((range (neomacs-perf--json-get diagnostic "range"))
             (start (neomacs-perf--lsp-position
                     (neomacs-perf--json-get range "start")))
             (end (max (1+ start)
                       (neomacs-perf--lsp-position
                        (neomacs-perf--json-get range "end"))))
             (overlay (make-overlay start end nil nil t)))
        (overlay-put overlay 'neomacs-perf-lsp t)
        (overlay-put overlay 'lsp-diagnostic diagnostic)
        (overlay-put overlay 'face
                     (neomacs-perf--diagnostic-face
                      (neomacs-perf--json-get diagnostic "severity")))
        (overlay-put overlay 'help-echo
                     (neomacs-perf--json-get diagnostic "message"))))))

(defun neomacs-perf--lsp-diagnostic-count (workspace)
  (seq-reduce (lambda (count diagnostics)
                (+ count (length diagnostics)))
              (hash-table-values (lsp--workspace-diagnostics workspace))
              0))

(defun neomacs-perf--overlay-count ()
  (length
   (seq-filter (lambda (overlay)
                 (overlay-get overlay 'neomacs-perf-lsp))
               (overlays-in (point-min) (point-max)))))

(defun neomacs-perf--run ()
  (let* ((result-path (neomacs-perf--required-environment
                        "NEOMACS_PERF_RESULT"))
          (sentinel-path (neomacs-perf--required-environment "SENTINEL"))
          (source-path (neomacs-perf--required-environment
                        "NEOMACS_PERF_SOURCE"))
          (replay-path (neomacs-perf--required-environment
                        "NEOMACS_PERF_LSP_REPLAY"))
          (grammar-path (neomacs-perf--required-environment
                         "NEOMACS_PERF_TREE_SITTER_DIR"))
          (iterations (string-to-number
                       (neomacs-perf--required-environment
                        "NEOMACS_PERF_ITERATIONS")))
          (elapsed-us 0)
          (major-mode-name "uninitialized")
          (parser-language "unavailable")
          (text-unchanged nil)
          (point-unchanged nil)
          (overlay-count 0)
          (lsp-diagnostic-count 0)
          (status "error")
          (error-message nil)
          (exit-code 2))
     (condition-case error-data
         (progn
           (unless (> iterations 0)
             (error "iterations must be positive"))
           (setq treesit-extra-load-path (list grammar-path))
           (unless (treesit-language-available-p 'rust)
             (error "the Rust Tree-sitter grammar is unavailable"))
           (let* ((buffer (find-file-noselect source-path))
                  (replay-json
                   (with-temp-buffer
                     (insert-file-contents replay-path)
                     (buffer-string)))
                  (workspace (make-lsp--workspace))
                  (lsp-diagnostic-stats (ht)))
             (set-window-buffer (selected-window) buffer)
             (with-current-buffer buffer
               (rust-ts-mode)
               (when (fboundp 'display-line-numbers-mode)
                 (display-line-numbers-mode 1))
               (font-lock-ensure)
               (goto-char (point-min))
               (search-forward "    bin_dir: PathBuf,")
               (search-backward ",")
               (let ((initial-text (buffer-string))
                     (initial-point (point)))
                 (neomacs-perf--apply-diagnostic-replay workspace replay-json)
                 (redisplay t)
                 (let ((sampling-enabled nil))
                   (neomacs-perf--sampling-command "enable")
                   (setq sampling-enabled t)
                   (unwind-protect
                       (let ((started (car (current-cpu-time))))
                         (dotimes (_ iterations)
                           (self-insert-command 1 ?j)
                           (neomacs-perf--apply-diagnostic-replay workspace replay-json)
                           (font-lock-ensure (line-beginning-position)
                                             (line-end-position))
                           (redisplay t)
                           (delete-char -1)
                           (neomacs-perf--apply-diagnostic-replay workspace replay-json)
                           (font-lock-ensure (line-beginning-position)
                                             (line-end-position))
                           (redisplay t))
                         (setq elapsed-us
                               (- (car (current-cpu-time)) started)))
                     (when sampling-enabled
                       (neomacs-perf--sampling-command "disable"))))
                 (setq major-mode-name (symbol-name major-mode)
                       parser-language
                       (symbol-name
                        (treesit-parser-language
                         (car (treesit-parser-list))))
                       text-unchanged (equal initial-text (buffer-string))
                       point-unchanged (= initial-point (point))
                       overlay-count (neomacs-perf--overlay-count)
                       lsp-diagnostic-count
                       (neomacs-perf--lsp-diagnostic-count workspace))))
           (setq status "ok"
                 exit-code 0)))
       (error
       (setq error-message (error-message-string error-data))
       (message "rust-lsp-typing failed: %s" error-message)))
     (neomacs-perf--close-profile-gate)
     (neomacs-perf--write-result
      result-path status iterations elapsed-us major-mode-name parser-language
      text-unchanged point-unchanged overlay-count lsp-diagnostic-count
      error-message)
     (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf--run)
  (run-at-time 0 nil #'neomacs-perf--run))

;;; rust-lsp-typing.el ends here
