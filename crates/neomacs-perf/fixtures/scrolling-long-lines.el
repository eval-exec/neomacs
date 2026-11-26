;;; scrolling-long-lines.el --- deterministic long-line truncated scroll workload  -*- lexical-binding: t; -*-

;; Pins the Issue #702 long-line scroll band identified in emacs-31.1
;; `git log -n 65536 --graph --decorate` lines 35729..44529:
;;   - 200x50 TUI window, `truncate-lines' = t, `word-wrap' = nil, `hl-line-mode' = 1
;;   - 30-column right margin with one `before-string' overlay per line
;;     carrying `(display ((margin right-margin) ...))'
;;   - 2,400 deterministic lines (120 x 20-line repeating band):
;;       exact mean length = 179.30 chars (sum 3586 / 20)
;;       55.0% of lines strictly > 200 columns (11 / 20)
;;       max line length   = 319 chars
;;     with alternating-face `| ' graph-lane prefixes matching Magit's
;;     colored merge-graph log lines.
;;
;; Scope boundary: runs in `fundamental-mode' with pre-propertized lines so
;; it isolates and guards the layout/truncation/horizon-retry/right-edge-marker/
;; overlay-route path (84.6% of instructions / 52.1% of cycles in Issue #702).
;; It intentionally does NOT run `jit-lock' fontification during scrolling and
;; therefore does not cover `RowsFontificationDemand::ensure' CoW/realloc.
;;
;; Construction runs outside timing; wall time brackets strictly the cold+warm
;; scroll phases. Works identically on GNU Emacs 31.1 and Neomacs with zero
;; external git or MELPA dependencies.

(require 'json)
(require 'hl-line)

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

(defconst neomacs-perf-long-lines--line-count 2400
  "Total physical lines (120 x 20-line repeating band).
Large enough for ~52 scroll-up screens per cold pass at 200x50.")

(defconst neomacs-perf-long-lines--band-lengths
  [72 206 80 212 88 218 92 226 96 234 100 242 104 250 112 262 120 272 281 319]
  "Exact 20-line length profile pinning the #702 emacs-31.1 band:
sum = 3586, mean = 179.30 chars, 11/20 (55.0%) > 200 cols, max = 319.")

(defconst neomacs-perf-long-lines--right-margin-width 30
  "Right-margin column width matching Magit log's author+age margin.")

(defconst neomacs-perf-long-lines--graph-faces
  [(:foreground "red")
   (:foreground "green")
   (:foreground "yellow")
   (:foreground "blue")
   (:foreground "magenta")
   (:foreground "cyan")]
  "Six alternating faces for per-lane `| ' graph prefixes.")

(defconst neomacs-perf-long-lines--hash-face '(:foreground "brightblack"))
(defconst neomacs-perf-long-lines--author-face '(:foreground "yellow"))
(defconst neomacs-perf-long-lines--date-face '(:foreground "cyan"))

(defun neomacs-perf-long-lines--make-margin-string (index)
  (let* ((author (format "Author %-11d" (% index 97)))
         (date (format "%2d years    " (1+ (% index 12))))
         (label (concat
                 (propertize author 'face neomacs-perf-long-lines--author-face)
                 (propertize date 'face neomacs-perf-long-lines--date-face))))
    (propertize "o" 'display (list '(margin right-margin) label))))

(defun neomacs-perf-long-lines--insert-line (index target-len)
  (let* ((line-start (point))
         (hash (format "%011x " (+ #x10000000000 index)))
         (subject (format "* src/xdisp.c: deterministic long-line entry %04d" index))
         (subject-len (length subject))
         (hash-len (length hash))
         (avail-graph (max 4 (- target-len hash-len subject-len)))
         (lanes (/ avail-graph 2))
         (pad-len (max 0 (- target-len hash-len (* lanes 2) subject-len))))
    (insert (propertize hash 'face neomacs-perf-long-lines--hash-face))
    (dotimes (lane lanes)
      (let ((face (aref neomacs-perf-long-lines--graph-faces (% lane 6))))
        (insert (propertize (if (= lane 1) "* " "| ") 'face face))))
    (insert subject)
    (when (> pad-len 0)
      (insert (make-string pad-len ?x)))
    (let ((actual-len (- (point) line-start)))
      (when (> actual-len target-len)
        (delete-region (+ line-start target-len) (point)))
      (when (< (- (point) line-start) target-len)
        (insert (make-string (- target-len (- (point) line-start)) ?x))))
    (insert "\n")
    (let ((ov (make-overlay line-start (point))))
      (overlay-put ov 'evaporate t)
      (overlay-put ov 'before-string
                   (neomacs-perf-long-lines--make-margin-string index)))))

(defun neomacs-perf-long-lines--insert-buffer ()
  (let ((n neomacs-perf-long-lines--line-count)
        (band neomacs-perf-long-lines--band-lengths)
        (band-len (length neomacs-perf-long-lines--band-lengths))
        (total-chars 0)
        (over-200 0)
        (max-len 0))
    (dotimes (i n)
      (let ((target (aref band (% i band-len))))
        (neomacs-perf-long-lines--insert-line i target)
        (setq total-chars (+ total-chars target))
        (when (> target 200)
          (setq over-200 (1+ over-200)))
        (when (> target max-len)
          (setq max-len target))))
    (setq-local truncate-lines t)
    (setq-local word-wrap nil)
    (setq-local right-margin-width neomacs-perf-long-lines--right-margin-width)
    (set-window-margins (selected-window) 0 neomacs-perf-long-lines--right-margin-width)
    (hl-line-mode 1)
    (let ((summary
           `((profile . "long-lines-702-v1")
             (lines . ,n)
             (overlays . ,n)
             (avg_line_length . ,(/ (float total-chars) n))
             (over_200_count . ,over-200)
             (over_200_percent . ,(/ (* 100.0 over-200) n))
             (max_line_length . ,max-len)
             (truncate_lines . t)
             (right_margin_width . ,neomacs-perf-long-lines--right-margin-width)
             (hl_line_mode . t))))
      (with-temp-file (concat (getenv "NEOMACS_PERF_RESULT") ".content.json")
        (insert (json-serialize summary :false-object :json-false))))))

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
        (scenario . "scrolling-long-lines")
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

(defun neomacs-perf--sampling-command-ack (command)
  "Return t only after the existing sampling gate acknowledges COMMAND."
  (unless (getenv "NEOMACS_PERF_GATE_PORT")
    (error "GC window diagnostics require an acknowledged sampling gate"))
  (neomacs-perf--sampling-command command)
  (unless (equal neomacs-perf--profile-gate-response "ack\n")
    (error "GC window diagnostics lack a %s acknowledgement" command))
  t)

(defun neomacs-perf--write-gc-window
    (path scenario iterations before-enable after-enable
          before-disable after-disable enable-ack disable-ack)
  (with-temp-file path
    (insert
     (json-serialize
      `((schema . "t34-edit-loop-gc-window-v1")
        (scenario . ,scenario)
        (iterations . ,iterations)
        (editor_pid . ,(emacs-pid))
        (gc_window . ((scope . "acknowledged-enable-to-disable")
                      (before_enable . ,before-enable)
                      (after_enable . ,after-enable)
                      (before_disable . ,before-disable)
                      (after_disable . ,after-disable)
                      (enable_ack . ,(if enable-ack t :json-false))
                      (disable_ack . ,(if disable-ack t :json-false)))))
      :false-object :json-false :null-object nil))))

(defun neomacs-perf-long-lines--run (&optional gc-window-path)
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
         (started-wall nil)
         (elapsed-wall-us 0)
         (gc-before-enable nil) (gc-after-enable nil)
         (gc-before-disable nil) (gc-after-disable nil)
         (gc-enable-ack nil) (gc-disable-ack nil)
         (status "error")
         (error-message nil)
         (exit-code 2))
    (condition-case error-data
        (progn
          (unless (> warm-passes 0)
            (error "iterations must be positive"))
          (switch-to-buffer (get-buffer-create "*neomacs-perf-scrolling-long-lines*"))
          (delete-other-windows)
          (erase-buffer)
          (neomacs-perf-long-lines--insert-buffer)
          (goto-char (point-min))
          (setq initial-point (point)
                initial-checksum
                (secure-hash
                 'sha256
                 (buffer-substring-no-properties (point-min) (point-max))))
          ;; Settle the initial display untimed: time SCROLLING, not setup.
          (redisplay t)
          (garbage-collect)
          (let ((sampling-enabled nil))
            (if gc-window-path
                (progn
                  (setq gc-before-enable gcs-done)
                  (setq gc-enable-ack
                        (neomacs-perf--sampling-command-ack "enable"))
                  (setq gc-after-enable gcs-done))
              (neomacs-perf--sampling-command "enable"))
            (setq sampling-enabled t)
            (unwind-protect
                (progn
                  ;; Capture wall time strictly around the cold + warm scroll
                  ;; phases (after buffer setup, initial redisplay, GC, and
                  ;; sampling-gate enable) so setup never leaks into
                  ;; `per-operation-wall-time'.
                  (setq started-wall (float-time))
                  ;; COLD pass: top to bottom over all 2,400 long truncated lines.
                  (let ((scroll-error-top-bottom t)
                        (start (neomacs-perf-scroll--cpu-us))
                        (safety 2000))
                    (while (and (not (neomacs-perf-scroll--at-end-p))
                                (> safety 0))
                      (setq this-command 'scroll-up-command
                            last-command 'scroll-up-command)
                      (run-hooks 'pre-command-hook)
                      (scroll-up-command nil)
                      (run-hooks 'post-command-hook)
                      (redisplay t)
                      (setq cold-commands (1+ cold-commands)
                            safety (1- safety)))
                    (when (zerop safety)
                      (error "cold pass never reached point-max"))
                    (setq cold-us (- (neomacs-perf-scroll--cpu-us) start)))
                  ;; WARM passes: down and back up over the same long-line band.
                  (let ((scroll-error-top-bottom t)
                        (start (neomacs-perf-scroll--cpu-us)))
                    (dotimes (_ warm-passes)
                      (goto-char (point-min))
                      (set-window-start nil (point-min))
                      (while (not (neomacs-perf-scroll--at-end-p))
                        (setq this-command 'scroll-up-command
                              last-command 'scroll-up-command)
                        (run-hooks 'pre-command-hook)
                        (scroll-up-command nil)
                        (run-hooks 'post-command-hook)
                        (redisplay t)
                        (setq warm-commands (1+ warm-commands)))
                      (while (not (neomacs-perf-scroll--at-start-p))
                        (setq this-command 'scroll-down-command
                              last-command 'scroll-down-command)
                        (run-hooks 'pre-command-hook)
                        (scroll-down-command nil)
                        (run-hooks 'post-command-hook)
                        (redisplay t)
                        (setq warm-commands (1+ warm-commands))))
                    (setq warm-us (- (neomacs-perf-scroll--cpu-us) start)))
                  (setq elapsed-wall-us
                        (round (* 1000000 (- (float-time) started-wall)))))
              (when sampling-enabled
                (if gc-window-path
                    (progn
                      (setq gc-before-disable gcs-done)
                      (setq gc-disable-ack
                            (neomacs-perf--sampling-command-ack "disable"))
                      (setq gc-after-disable gcs-done))
                  (neomacs-perf--sampling-command "disable")))))
          (setq status "ok"
                exit-code 0))
      (error
       (setq error-message (error-message-string error-data))
       (message "scrolling-long-lines failed: %s" error-message)))
    (let* ((buffer (get-buffer "*neomacs-perf-scrolling-long-lines*"))
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
              "")))
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
    (when gc-window-path
      (neomacs-perf--write-gc-window
       gc-window-path "scrolling-long-lines" warm-passes
       gc-before-enable gc-after-enable
       gc-before-disable gc-after-disable
       gc-enable-ack gc-disable-ack))
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(let ((gc-window-path (getenv "NEOMACS_PERF_GC_WINDOW_FILE")))
  (if noninteractive
      (neomacs-perf-long-lines--run gc-window-path)
    (run-at-time 0 nil #'neomacs-perf-long-lines--run gc-window-path)))

;;; scrolling-long-lines.el ends here
