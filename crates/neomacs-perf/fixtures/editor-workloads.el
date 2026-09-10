;;; editor-workloads.el --- correctness-gated editor workflows -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'json)

(defvar neomacs-perf-workload--gate-process nil)
(defvar neomacs-perf-workload--gate-response "")

(defun neomacs-perf-workload--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defun neomacs-perf-workload--gate-filter (_process output)
  (setq neomacs-perf-workload--gate-response
        (concat neomacs-perf-workload--gate-response output)))

(defun neomacs-perf-workload--sampling-command (command)
  (let ((port-text (getenv "NEOMACS_PERF_GATE_PORT")))
    (when port-text
      (unless (process-live-p neomacs-perf-workload--gate-process)
        (setq neomacs-perf-workload--gate-process
              (make-network-process
               :name "neomacs-perf-workload-gate"
               :family 'ipv4 :host "127.0.0.1"
               :service (string-to-number port-text)
               :coding 'binary :noquery t
               :filter #'neomacs-perf-workload--gate-filter)))
      (setq neomacs-perf-workload--gate-response "")
      (process-send-string neomacs-perf-workload--gate-process
                           (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and (not (string-suffix-p "\n" neomacs-perf-workload--gate-response))
                    (< (float-time) deadline))
          (unless (process-live-p neomacs-perf-workload--gate-process)
            (error "performance gate disconnected during %s" command))
          (accept-process-output neomacs-perf-workload--gate-process 0.05)))
      (unless (equal neomacs-perf-workload--gate-response "ack\n")
        (error "performance gate rejected %s: %S"
               command neomacs-perf-workload--gate-response)))))

(defun neomacs-perf-workload--cpu-us ()
  (car (current-cpu-time)))

(defun neomacs-perf-workload--checksum ()
  (secure-hash 'sha256 (current-buffer)))

(defun neomacs-perf-workload--time (function)
  (let ((started (neomacs-perf-workload--cpu-us)))
    (funcall function)
    (max 1 (- (neomacs-perf-workload--cpu-us) started))))

(defun neomacs-perf-workload--latency-time (function)
  (let ((started (float-time)))
    (funcall function)
    (max 1 (round (* 1000000 (- (float-time) started))))))

(defun neomacs-perf-workload--restore (text point)
  (unless (equal text (buffer-substring-no-properties (point-min) (point-max)))
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert text)))
  (goto-char (min point (point-max))))

(defun neomacs-perf-workload--single-edit-cycle ()
  (goto-char (point-max))
  (let ((last-command-event ?x))
    (call-interactively #'self-insert-command))
  (font-lock-ensure (line-beginning-position) (line-end-position))
  (redisplay t)
  (delete-char -1)
  (redisplay t))

(defun neomacs-perf-workload--type-phase ()
  (goto-char (point-max))
  (dolist (line '("(defun sim--generated (x y)"
                  "  \"Docstring for the generated function.\""
                  "  (let ((acc nil))"
                  "    (dotimes (i (+ x y))"
                  "      (push (* i i) acc))"
                  "    (nreverse acc)))"))
    (dolist (character (string-to-list line))
      (insert character)
      (font-lock-ensure (line-beginning-position) (line-end-position)))
    (insert "\n")))

(defun neomacs-perf-workload--comment-phase ()
  (goto-char (point-min))
  (forward-line 300)
  (let ((start (line-beginning-position)))
    (forward-line 100)
    (comment-region start (point))
    (uncomment-region start (point))))

(defun neomacs-perf-workload--kill-yank-phase ()
  (goto-char (point-min))
  (dotimes (_ 20)
    (goto-char (point-min))
    (forward-line 100)
    (let ((start (point)))
      (forward-line 50)
      (kill-region start (point))
      (goto-char (point-max))
      (yank))))

(defun neomacs-perf-workload--indent-phase ()
  (goto-char (point-min))
  (forward-line 200)
  (let ((start (point)))
    (forward-line 400)
    (indent-region start (point))))

(defun neomacs-perf-workload--regex-phase ()
  (goto-char (point-min))
  (let ((matches 0))
    (while (re-search-forward "(defun \\([-a-z0-9]+\\)" nil t)
      (setq matches (1+ matches)))
    matches))

(defun neomacs-perf-workload--search-phase ()
  (dotimes (_ 50)
    (neomacs-perf-workload--regex-phase)))

(defun neomacs-perf-workload--replace-phase ()
  (goto-char (point-min))
  (while (re-search-forward "\\_<byte-compile\\_>" nil t)
    (replace-match "byte-compile" t t))
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-perf-workload--undo-redo-phase ()
  (buffer-enable-undo)
  (dotimes (_ 10)
    (goto-char (point-max))
    (let ((start (point)))
      (dolist (line '("(defun sim--undo-probe (n)" "  (* n n))"))
        (insert line "\n"))
      (undo-boundary)
      (delete-region start (point-max))
      (undo-boundary))
    (primitive-undo 2 buffer-undo-list)))

(defun neomacs-perf-workload--isearch-phase ()
  (let ((needles '("d" "de" "def" "defu" "defun" "defun ")))
    (dotimes (_ 5)
      (dolist (needle needles)
        (goto-char (point-min))
        (while (search-forward needle nil t))))))

(defun neomacs-perf-workload--buffer-switch-phase ()
  (let ((buffers
         (cl-loop for index below 8
                  collect
                  (let ((buffer (generate-new-buffer
                                 (format " sim-buf-%d" index))))
                    (with-current-buffer buffer
                      (insert (format ";; buffer %d\n(defvar sim-var-%d %d)\n"
                                      index index index))
                      (emacs-lisp-mode))
                    buffer))))
    (unwind-protect
        (dotimes (_ 200)
          (dolist (buffer buffers)
            (with-current-buffer buffer
              (goto-char (point-min))
              (forward-line 1)
              (end-of-line))))
      (mapc #'kill-buffer buffers))))

(defun neomacs-perf-workload--how-many-phase ()
  (goto-char (point-min))
  (while (re-search-forward "\\_<let\\*?\\_>" nil t)))

(defun neomacs-perf-workload--motion-phase ()
  (goto-char (point-min))
  (while (not (eobp))
    (forward-line 1)
    (end-of-line)
    (beginning-of-line)))

(defun neomacs-perf-workload--prepare-buffer (scenario)
  (cond
   ((equal scenario "startup")
    (fundamental-mode))
   ((equal scenario "org-editing")
    (require 'org)
    (dotimes (section 150)
      (insert (format "* TODO Section %d\n:PROPERTIES:\n:ID: item-%d\n:END:\n"
                      section section)
              "| Name | Value |\n|------+-------|\n| alpha | 1 |\n\n"))
    (org-mode))
   ((equal scenario "magit-status")
    (require 'magit)
    (magit-status-setup-buffer
     (neomacs-perf-workload--required-environment "NEOMACS_PERF_REPOSITORY")))
   (t
    (insert-file-contents
     (neomacs-perf-workload--required-environment "NEOMACS_PERF_SOURCE"))
    (emacs-lisp-mode)))
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-perf-workload--execute (scenario iterations)
  (let ((type-us 0) (comment-us 0) (kill-yank-us 0)
        (indent-us 0) (regex-us 0) (latencies nil)
        (mode-us 0) (fontify-us 0) (replace-us 0) (undo-redo-us 0)
        (isearch-us 0) (buffer-switch-us 0) (how-many-us 0) (motion-us 0))
    (dotimes (_ iterations)
      (let ((text (buffer-substring-no-properties (point-min) (point-max)))
            (saved-point (point)))
        (pcase scenario
          ("editing-simulation"
           (setq mode-us (+ mode-us (neomacs-perf-workload--time
                                     #'emacs-lisp-mode))
                 fontify-us (+ fontify-us (neomacs-perf-workload--time
                                           (lambda ()
                                             (font-lock-ensure
                                              (point-min) (point-max)))))
                 regex-us (+ regex-us (neomacs-perf-workload--time
                                       #'neomacs-perf-workload--search-phase))
                 type-us (+ type-us (neomacs-perf-workload--time
                                     #'neomacs-perf-workload--type-phase))
                 replace-us (+ replace-us (neomacs-perf-workload--time
                                           #'neomacs-perf-workload--replace-phase))
                 indent-us (+ indent-us (neomacs-perf-workload--time
                                         #'neomacs-perf-workload--indent-phase))
                 kill-yank-us (+ kill-yank-us (neomacs-perf-workload--time
                                               #'neomacs-perf-workload--kill-yank-phase))
                 undo-redo-us (+ undo-redo-us (neomacs-perf-workload--time
                                               #'neomacs-perf-workload--undo-redo-phase))
                 isearch-us (+ isearch-us (neomacs-perf-workload--time
                                           #'neomacs-perf-workload--isearch-phase))
                 buffer-switch-us (+ buffer-switch-us (neomacs-perf-workload--time
                                                       #'neomacs-perf-workload--buffer-switch-phase))
                 comment-us (+ comment-us (neomacs-perf-workload--time
                                           #'neomacs-perf-workload--comment-phase))
                 how-many-us (+ how-many-us (neomacs-perf-workload--time
                                             #'neomacs-perf-workload--how-many-phase))
                 motion-us (+ motion-us (neomacs-perf-workload--time
                                         #'neomacs-perf-workload--motion-phase))))
          ("startup" (redisplay t))
          ("sustained-editing"
           (setq type-us (+ type-us (neomacs-perf-workload--time
                                     #'neomacs-perf-workload--single-edit-cycle))))
          ("gui-input-latency"
           (push (neomacs-perf-workload--latency-time
                  (lambda ()
                    (goto-char (point-max))
                    (let ((last-command-event ?x))
                      (call-interactively #'self-insert-command))
                    (font-lock-ensure (line-beginning-position) (line-end-position))
                    (redisplay t)))
                 latencies)
           (delete-char -1)
           (redisplay t))
          ("org-editing"
           (setq type-us (+ type-us (neomacs-perf-workload--time
                                     (lambda ()
                                       (goto-char (point-min))
                                       (re-search-forward "^\\* TODO ")
                                       (org-todo "DONE")
                                       (re-search-forward "^| Name |")
                                       (org-table-align)
                                       (font-lock-ensure
                                        (point-min) (point-max)))))))
          ("magit-status"
           (setq regex-us (+ regex-us (neomacs-perf-workload--time
                                       #'magit-refresh))))
          ("large-file-editing"
           (setq type-us (+ type-us (neomacs-perf-workload--time
                                     #'neomacs-perf-workload--type-phase))
                 regex-us (+ regex-us (neomacs-perf-workload--time
                                       #'neomacs-perf-workload--regex-phase))
                 motion-us (+ motion-us (neomacs-perf-workload--time
                                         #'neomacs-perf-workload--motion-phase))))
          ("indentation"
           (setq indent-us (+ indent-us (neomacs-perf-workload--time
                                         #'neomacs-perf-workload--indent-phase))))
          ("regex-search"
           (setq regex-us (+ regex-us (neomacs-perf-workload--time
                                       #'neomacs-perf-workload--regex-phase))))
          (_ (error "unknown editor workload %S" scenario)))
        (neomacs-perf-workload--restore text saved-point)))
    `((type . ,type-us)
      (comment . ,comment-us)
      (kill-yank . ,kill-yank-us)
      (indent . ,indent-us)
      (regex . ,regex-us)
      (latencies . ,(vconcat (nreverse latencies)))
      (mode . ,mode-us)
      (fontify . ,fontify-us)
      (replace . ,replace-us)
      (undo-redo . ,undo-redo-us)
      (isearch . ,isearch-us)
      (buffer-switch . ,buffer-switch-us)
      (how-many . ,how-many-us)
      (motion . ,motion-us))))

(defun neomacs-perf-workload--max-rss-kb ()
  "Peak resident set size in kB, or 0 where the kernel does not report it.
Both engines run the same code here, so the number is comparable."
  (if (file-readable-p "/proc/self/status")
      (with-temp-buffer
        (insert-file-contents "/proc/self/status")
        (goto-char (point-min))
        (if (re-search-forward "^VmHWM:[ \t]*\\([0-9]+\\)" nil t)
            (string-to-number (match-string 1))
          0))
    0))

(defun neomacs-perf-workload--write-result
    (path scenario status iterations elapsed-us elapsed-wall-us operation-count
          initial-checksum final-checksum point-restored expected-mode actual-mode
          phases error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1) (scenario . ,scenario) (status . ,status)
        (iterations . ,iterations) (elapsed_us . ,elapsed-us)
        (elapsed_wall_us . ,elapsed-wall-us)
        (operation_count . ,operation-count)
        (initial_checksum . ,initial-checksum) (final_checksum . ,final-checksum)
        (point_restored . ,(if point-restored t :json-false))
        (expected_major_mode . ,expected-mode) (actual_major_mode . ,actual-mode)
        (type_phase_us . ,(alist-get 'type phases))
        (comment_phase_us . ,(alist-get 'comment phases))
        (kill_yank_phase_us . ,(alist-get 'kill-yank phases))
        (indent_phase_us . ,(alist-get 'indent phases))
        (regex_phase_us . ,(alist-get 'regex phases))
        (latency_samples_us . ,(alist-get 'latencies phases))
        (mode_phase_us . ,(alist-get 'mode phases))
        (fontify_phase_us . ,(alist-get 'fontify phases))
        (replace_phase_us . ,(alist-get 'replace phases))
        (undo_redo_phase_us . ,(alist-get 'undo-redo phases))
        (isearch_phase_us . ,(alist-get 'isearch phases))
        (buffer_switch_phase_us . ,(alist-get 'buffer-switch phases))
        (how_many_phase_us . ,(alist-get 'how-many phases))
        (motion_phase_us . ,(alist-get 'motion phases))
        ;; Collection parity. Comparing two engines without these is comparing
        ;; different amounts of work: neomacs performs no automatic collections
        ;; in --batch (its adaptive pacer's live-growth term is a strict max
        ;; over gc-cons-threshold), so a batch row silently charges GNU for
        ;; collection the other engine skipped.
        (gcs_done . ,gcs-done)
        (gc_elapsed_us . ,(round (* 1000000 gc-elapsed)))
        (max_rss_kb . ,(neomacs-perf-workload--max-rss-kb))
        (error . ,error-message))
      :false-object :json-false :null-object nil))))

(defun neomacs-perf-workload--run ()
  (let* ((scenario (neomacs-perf-workload--required-environment "NEOMACS_PERF_WORKLOAD"))
         (iterations (string-to-number
                      (neomacs-perf-workload--required-environment
                       "NEOMACS_PERF_ITERATIONS")))
         (result-path (neomacs-perf-workload--required-environment
                       "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf-workload--required-environment "SENTINEL"))
         (status "error") (error-message nil) (exit-code 2)
         (elapsed-us 0) (elapsed-wall-us 0)
         (initial-checksum "") (final-checksum "")
         (initial-point 1) (point-restored nil) (expected-mode "")
         (actual-mode "")
         (phases '((type . 0) (comment . 0) (kill-yank . 0)
                   (indent . 0) (regex . 0) (latencies . [])
                   (mode . 0) (fontify . 0) (replace . 0)
                   (undo-redo . 0) (isearch . 0) (buffer-switch . 0)
                   (how-many . 0) (motion . 0))))
    (condition-case error-data
        (with-temp-buffer
          (neomacs-perf-workload--prepare-buffer scenario)
          (setq expected-mode (symbol-name major-mode)
                initial-checksum (neomacs-perf-workload--checksum)
                initial-point (point))
          (garbage-collect)
          (neomacs-perf-workload--sampling-command "enable")
          (let ((started (neomacs-perf-workload--cpu-us))
                (wall-started (float-time)))
            (unwind-protect
                (setq phases (neomacs-perf-workload--execute scenario iterations)
                      elapsed-us (max 1 (- (neomacs-perf-workload--cpu-us) started))
                      elapsed-wall-us
                      (max 1 (round (* 1000000 (- (float-time) wall-started)))))
              (neomacs-perf-workload--sampling-command "disable")))
          (setq final-checksum (neomacs-perf-workload--checksum)
                point-restored (= (point) initial-point)
                actual-mode (symbol-name major-mode)
                status "ok" exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "%s failed: %s" scenario error-message)))
    (when (processp neomacs-perf-workload--gate-process)
      (delete-process neomacs-perf-workload--gate-process))
    (neomacs-perf-workload--write-result
     result-path scenario status iterations elapsed-us elapsed-wall-us iterations
     initial-checksum final-checksum point-restored expected-mode actual-mode phases
     error-message)
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-workload--run)
  (run-at-time 0 nil #'neomacs-perf-workload--run))

;;; editor-workloads.el ends here
