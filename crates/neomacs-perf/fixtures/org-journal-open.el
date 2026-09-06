;;; org-journal-open.el --- correctness-gated org-journal yearly-file open -*- lexical-binding: t; -*-

;; Reproduces the real-user workload behind the 2026-09-01 and 2026-09-06
;; org-journal stall investigations: open a yearly org-journal file through
;; `org-journal-new-entry' with org-superstar bullet fontification and
;; global git-gutter overlays active, then let font-lock settle over the
;; whole buffer.  The user configuration being mirrored (see
;; docs/performance/2026-09-01-org-journal-overlay-redisplay.md and the
;; private config it references) is replicated here under -Q; the network
;; weather hook from that config is deliberately not part of the workload.

(require 'cl-lib)
(require 'json)

(defvar neomacs-perf-journal--gate-process nil)
(defvar neomacs-perf-journal--gate-response "")

(defun neomacs-perf-journal--required-environment (name)
  (or (getenv name)
      (error "required performance environment variable %s is absent" name)))

(defun neomacs-perf-journal--gate-filter (_process output)
  (setq neomacs-perf-journal--gate-response
        (concat neomacs-perf-journal--gate-response output)))

(defun neomacs-perf-journal--sampling-command (command)
  (let ((port-text (getenv "NEOMACS_PERF_GATE_PORT")))
    (when port-text
      (unless (process-live-p neomacs-perf-journal--gate-process)
        (setq neomacs-perf-journal--gate-process
              (make-network-process
               :name "neomacs-perf-journal-gate"
               :family 'ipv4 :host "127.0.0.1"
               :service (string-to-number port-text)
               :coding 'binary :noquery t
               :filter #'neomacs-perf-journal--gate-filter)))
      (setq neomacs-perf-journal--gate-response "")
      (process-send-string neomacs-perf-journal--gate-process
                           (concat command "\n"))
      (let ((deadline (+ (float-time) 30.0)))
        (while (and (not (string-suffix-p "\n" neomacs-perf-journal--gate-response))
                    (< (float-time) deadline))
          (unless (process-live-p neomacs-perf-journal--gate-process)
            (error "performance gate disconnected during %s" command))
          (accept-process-output neomacs-perf-journal--gate-process 0.05)))
      (unless (equal neomacs-perf-journal--gate-response "ack\n")
        (error "performance gate rejected %s: %S"
               command neomacs-perf-journal--gate-response)))))

(defun neomacs-perf-journal--cpu-us ()
  (car (current-cpu-time)))

(defun neomacs-perf-journal--time (function)
  (let ((started (neomacs-perf-journal--cpu-us)))
    (funcall function)
    (max 1 (- (neomacs-perf-journal--cpu-us) started))))

(defun neomacs-perf-journal--configure ()
  "Replicate the org-journal path of the mirrored user configuration."
  (require 'org-journal)
  (require 'org-superstar)
  (require 'git-gutter)
  (setq org-journal-dir
        (neomacs-perf-journal--required-environment "NEOMACS_PERF_JOURNAL_DIR")
        org-journal-date-format "%F, %A"
        org-journal-time-format "%T "
        org-journal-file-format "%Y.org"
        org-journal-file-type 'yearly
        org-journal-enable-agenda-integration nil
        org-journal-enable-cache t
        org-journal-carryover-items ""
        org-journal-prefix-key nil
        org-journal-find-file-fn 'find-file
        org-superstar-leading-bullet ?\s)
  (add-hook 'org-mode-hook #'org-superstar-mode)
  (global-git-gutter-mode 1))

(defun neomacs-perf-journal--kill-journal-buffers (directory)
  "Kill every buffer visiting a file below DIRECTORY."
  (let ((prefix (file-name-as-directory directory)))
    (dolist (buffer (buffer-list))
      (let ((file (buffer-file-name buffer)))
        (when (and file
                   (string-prefix-p prefix (file-name-directory file)))
          (kill-buffer buffer))))))

(defun neomacs-perf-journal--overlay-count ()
  (length (overlays-in (point-min) (point-max))))

(defun neomacs-perf-journal--reset-journal (journal-file base-content)
  "Restore the yearly file to BASE-CONTENT for the next open cycle.

`org-journal-new-entry' is deliberately not idempotent -- every call
adds a new timed sub-entry under today's heading -- so each measured
cycle starts from the pristine base the harness committed.  Buffers are
killed without saving first; on-disk state is then overwritten from the
recorded base content."
  (neomacs-perf-journal--kill-journal-buffers org-journal-dir)
  (with-temp-file journal-file
    (insert base-content)))

(defun neomacs-perf-journal--stable-checksum ()
  "Checksum of the current buffer with per-entry timestamps removed.

`org-journal-time-format' embeds the wall clock in every new sub-entry,
so raw content can never be stable across cycles; everything else in a
cycle from a pristine base is deterministic."
  (secure-hash
   'sha256
   (replace-regexp-in-string "[0-2][0-9]:[0-5][0-9]:[0-5][0-9]" ""
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))

(defun neomacs-perf-journal--today-heading-present-p ()
  "Return non-nil when the current buffer contains today's journal heading."
  (save-excursion
    (goto-char (point-min))
    (search-forward (format-time-string org-journal-date-format) nil t)))

(defun neomacs-perf-journal--file-today-heading-present-p (file)
  "Return non-nil when FILE exists and already contains today's heading."
  (and (file-exists-p file)
       (with-temp-buffer
         (insert-file-contents file)
         (neomacs-perf-journal--today-heading-present-p))))

(defun neomacs-perf-journal--one-open ()
  "Run one journal-open cycle and return its observation alist.

The cycle reopens the yearly file through `org-journal-new-entry'
(creating today's entry when the file does not have one yet),
fontifies the whole buffer, and lets redisplay settle.  The observation
records the three phase times, whether this cycle created today's
entry, the post-settle overlay count, and the timestamp-normalized
post-settle buffer checksum."
  (let* ((journal-file (expand-file-name
                        (format-time-string org-journal-file-format)
                        org-journal-dir))
         (entry-existed (neomacs-perf-journal--file-today-heading-present-p
                         journal-file))
         (open-us (neomacs-perf-journal--time
                   (lambda () (org-journal-new-entry nil))))
         (buffer (current-buffer))
         (fontify-us (neomacs-perf-journal--time
                      (lambda ()
                        (with-current-buffer buffer
                          (font-lock-ensure (point-min) (point-max))))))
         (settle-us (neomacs-perf-journal--time
                     (lambda ()
                       (with-current-buffer buffer (redisplay t)))))
         (entry-created (with-current-buffer buffer
                          (and (not entry-existed)
                               (neomacs-perf-journal--today-heading-present-p))))
         (overlays (with-current-buffer buffer
                     (neomacs-perf-journal--overlay-count)))
         (line-count (with-current-buffer buffer
                       (count-lines (point-min) (point-max))))
         (checksum (with-current-buffer buffer
                     (neomacs-perf-journal--stable-checksum)))
         (superstar (with-current-buffer buffer
                      (and (boundp 'org-superstar-mode) org-superstar-mode)))
         (gutter (with-current-buffer buffer
                   (and (boundp 'git-gutter-mode) git-gutter-mode)))
         (mode (with-current-buffer buffer major-mode)))
    `((open-us . ,open-us)
      (fontify-us . ,fontify-us)
      (settle-us . ,settle-us)
      (entry-created . ,entry-created)
      (overlays . ,overlays)
      (line-count . ,line-count)
      (checksum . ,checksum)
      (superstar . ,superstar)
      (gutter . ,gutter)
      (mode . ,(symbol-name mode)))))

(defun neomacs-perf-journal--write-result
    (path status iterations elapsed-us elapsed-wall-us
          open-us fontify-us settle-us expected-mode actual-mode
          superstar gutter overlay-count-min overlay-count-final
          stable-checksum entry-created line-count error-message)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema_version . 1)
        (scenario . "org-journal-open")
        (status . ,status)
        (iterations . ,iterations)
        (elapsed_us . ,elapsed-us)
        (elapsed_wall_us . ,elapsed-wall-us)
        (operation_count . ,iterations)
        (open_phase_us . ,open-us)
        (fontify_phase_us . ,fontify-us)
        (settle_phase_us . ,settle-us)
        (expected_major_mode . ,expected-mode)
        (actual_major_mode . ,actual-mode)
        (org_superstar_active . ,(if superstar t :json-false))
        (git_gutter_active . ,(if gutter t :json-false))
        (overlay_count_min . ,overlay-count-min)
        (overlay_count_final . ,overlay-count-final)
        (stable_checksum . ,(if stable-checksum t :json-false))
        (entry_created . ,(if entry-created t :json-false))
        (journal_line_count . ,line-count)
        (error . ,error-message))
      :false-object :json-false :null-object nil))))

(defun neomacs-perf-journal--run ()
  (let* ((iterations (string-to-number
                      (neomacs-perf-journal--required-environment
                       "NEOMACS_PERF_ITERATIONS")))
         (result-path (neomacs-perf-journal--required-environment
                       "NEOMACS_PERF_RESULT"))
         (sentinel-path (neomacs-perf-journal--required-environment "SENTINEL"))
         (status "error") (error-message nil) (exit-code 2)
         (elapsed-us 0) (elapsed-wall-us 0)
         (open-us 0) (fontify-us 0) (settle-us 0)
         (expected-mode "org-journal-mode") (actual-mode "")
         (superstar nil) (gutter nil)
         (overlay-count-min most-positive-fixnum) (overlay-count-final 0)
         (stable-checksum t) (entry-created nil) (line-count 0)
         (first-checksum nil))
    (condition-case error-data
        (progn
          (neomacs-perf-journal--configure)
          (let* ((journal-file (expand-file-name
                                (format-time-string org-journal-file-format)
                                org-journal-dir))
                 (base-content (with-temp-buffer
                                 (insert-file-contents journal-file)
                                 (buffer-string))))
            (garbage-collect)
            (neomacs-perf-journal--sampling-command "enable")
            (let ((started (neomacs-perf-journal--cpu-us))
                  (wall-started (float-time)))
              (unwind-protect
                  (dotimes (_ iterations)
                    ;; `org-journal-new-entry' is not idempotent, so every
                    ;; cycle restarts from the pristine base: identical
                    ;; work per cycle, comparable phase times.
                    (neomacs-perf-journal--reset-journal
                     journal-file base-content)
                    (let ((observation (neomacs-perf-journal--one-open))
                          (checksum nil))
                      (setq open-us (+ open-us (alist-get 'open-us observation))
                            fontify-us (+ fontify-us
                                          (alist-get 'fontify-us observation))
                            settle-us (+ settle-us
                                         (alist-get 'settle-us observation))
                            actual-mode (alist-get 'mode observation)
                            superstar (alist-get 'superstar observation)
                            gutter (alist-get 'gutter observation)
                            overlay-count-final (alist-get 'overlays observation)
                            overlay-count-min (min overlay-count-min
                                                   overlay-count-final)
                            line-count (alist-get 'line-count observation)
                            checksum (alist-get 'checksum observation))
                      (when (alist-get 'entry-created observation)
                        (setq entry-created t))
                      (cond
                       ((null first-checksum)
                        ;; The first cycle establishes the post-settle
                        ;; reference for every later cycle.
                        (setq first-checksum checksum))
                       ((not (equal checksum first-checksum))
                        (setq stable-checksum nil)))))
                (neomacs-perf-journal--sampling-command "disable"))
              (setq elapsed-us (max 1 (- (neomacs-perf-journal--cpu-us) started))
                    elapsed-wall-us
                    (max 1 (round (* 1000000
                                     (- (float-time) wall-started))))
                    status "ok" exit-code 0))))
      (error
       (setq error-message (error-message-string error-data))
       (message "org-journal-open failed: %s" error-message)))
    (when (= overlay-count-min most-positive-fixnum)
      (setq overlay-count-min 0))
    (when (processp neomacs-perf-journal--gate-process)
      (delete-process neomacs-perf-journal--gate-process))
    (neomacs-perf-journal--write-result
     result-path status iterations elapsed-us elapsed-wall-us
     open-us fontify-us settle-us expected-mode actual-mode
     superstar gutter overlay-count-min overlay-count-final
     stable-checksum entry-created line-count error-message)
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-journal--run)
  (run-at-time 0 nil #'neomacs-perf-journal--run))

;;; org-journal-open.el ends here
