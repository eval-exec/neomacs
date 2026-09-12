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

(defvar neomacs-perf-workload--lsp-payload nil
  "The `textDocument/publishDiagnostics' plist the lsp-json-rpc row round-trips.")

(defvar neomacs-perf-workload--latency-trace nil
  "Reverse-ordered per-keystroke records, when latency tracing is on.")

(defun neomacs-perf-workload--latency-time (function)
  "Wall-clock microseconds spent in FUNCTION: one input-to-redisplay sample.

When NEOMACS_PERF_LATENCY_TRACE_FILE names a path, also record what that
wall time was made of.  A keystroke whose wall time is CPU time is doing
work a profiler can attribute; one whose wall time is not is waiting on
something outside this process -- the compositor, the GPU queue, the page
cache, the scheduler -- and no amount of profiling the editor will find
it.  Percentiles alone cannot tell those two tails apart, and they call
for opposite work.  Collection is broken out because it is the usual
first suspect and deserves to be confirmed or cleared by number."
  (let ((started-wall (float-time))
        (started-cpu (neomacs-perf-workload--cpu-us))
        (started-gcs gcs-done)
        (started-gc-us (round (* 1000000 gc-elapsed))))
    (funcall function)
    (let ((wall-us (max 1 (round (* 1000000 (- (float-time) started-wall))))))
      (when (getenv "NEOMACS_PERF_LATENCY_TRACE_FILE")
        (push (list wall-us
                    (- (neomacs-perf-workload--cpu-us) started-cpu)
                    (- gcs-done started-gcs)
                    (- (round (* 1000000 gc-elapsed)) started-gc-us)
                    (round (* 1000000 started-wall)))
              neomacs-perf-workload--latency-trace))
      wall-us)))

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

(defconst neomacs-perf-workload--org-subjects
  ["release prep" "design review" "reading log" "bug hunt" "pairing session"
   "refactor pass" "research spike" "profiling notes" "triage" "retro"]
  "Heading subjects, cycled so the document is deterministic.")

(defun neomacs-perf-workload--insert-org-document (sections)
  "Insert SECTIONS of Org markup of the kind a real Org file carries.

The `org-editing' row builds headings, property drawers and tables and
nothing else, so its per-iteration `font-lock-ensure\' over the whole
buffer never reaches Org\'s expensive matchers: `org-activate-links\',
`org-do-emphasis-faces\' and `org-fontify-meta-lines-and-blocks\' have
nothing to match.  Measured against the Org manual (doc/misc/org.org,
23,370 lines), that fixture has 0 links, 0 emphasis markers, 0 `#+\'
lines, 0 list items and 0 timestamps per 100 lines where the manual has
2.3, 19.3, 15.5, 6.1 and 0.2 -- while carrying 15x the manual\'s table
rows and 6.6x its property drawers.

This emits the same 150 TODO headings and keeps a table in every section,
so the timed operation is unchanged and the two rows stay comparable.
What differs is everything around them: tags, a scheduled timestamp, a
paragraph carrying two links and three emphasis forms, a checkbox list
item, and a source block.  Cycled from constants and indexed by section,
so every machine builds a byte-identical document.

Every section carries every construct, which makes this an UPPER bound
rather than an average file.  Measured per 100 lines against the manual,
`#+\' lines (14.3 vs 15.5) and list items (7.1 vs 6.1) land where the
manual has them, while links (21.4 vs 2.3), emphasis (35.7 vs 19.3) and
timestamps (7.1 vs 0.2) run above it.  The plain row is the lower bound --
zero of all of them -- so a real file sits between the two rows, which is
the point of keeping both."
  (dotimes (section sections)
    (insert
     (format "* TODO %s %d  :work:proj%d:\n"
             (aref neomacs-perf-workload--org-subjects
                   (mod section (length neomacs-perf-workload--org-subjects)))
             section (mod section 7))
     (format "SCHEDULED: <2026-09-%02d Sat>\n" (1+ (mod section 28)))
     (format ":PROPERTIES:\n:ID: item-%d\n:END:\n" section)
     (format (concat "See [[file:notes.org::*Section %d][the note]] and"
                     " [[https://example.invalid/%d][upstream]] for *context*;"
                     " the /interesting/ part is ~code~ plus =verbatim=.\n")
             section section)
     (format "- [ ] follow up on *item %d* with the [[https://example.invalid/a][owner]]\n"
             section)
     (format "#+BEGIN_SRC emacs-lisp\n(defun sim--section-%d (x) (* x x))\n#+END_SRC\n"
             section)
     "| Name | Value |\n|------+-------|\n| alpha | 1 |\n\n")))

(defun neomacs-perf-workload--prepare-buffer (scenario)
  (cond
   ((equal scenario "startup")
    (fundamental-mode))
   ((equal scenario "process-output")
    ;; The workload spawns the subprocess itself, once per iteration.
    (fundamental-mode))
   ((equal scenario "file-open")
    ;; The workload opens the file itself, once per iteration, into a fresh
    ;; buffer -- so this one starts empty.
    (fundamental-mode))
   ((equal scenario "lsp-json-rpc")
    ;; Build the payload once: the workload measures the round trip, not the
    ;; construction of the object graph.
    (setq neomacs-perf-workload--lsp-payload
          (neomacs-perf-workload--lsp-message 120))
    (fundamental-mode))
   ((equal scenario "org-editing")
    (require 'org)
    (dotimes (section 150)
      (insert (format "* TODO Section %d\n:PROPERTIES:\n:ID: item-%d\n:END:\n"
                      section section)
              "| Name | Value |\n|------+-------|\n| alpha | 1 |\n\n"))
    (org-mode))
   ((equal scenario "org-editing-heavy")
    (require 'org)
    (neomacs-perf-workload--insert-org-document 150)
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
        (isearch-us 0) (buffer-switch-us 0) (how-many-us 0) (motion-us 0)
        (bookkeeping-us 0))
    (dotimes (_ iterations)
      ;; The snapshot and the restore below are the HARNESS's own work, not
      ;; the workload's, and they sit inside the window `elapsed-us' and
      ;; `elapsed-wall-us' measure.  Two whole-buffer
      ;; `buffer-substring-no-properties' calls plus an `equal' per iteration
      ;; is not free, and it is not charged equally to the two engines, so
      ;; report what it cost instead of leaving it folded invisibly into
      ;; every row's primary metric.
      (let* ((book-started (float-time))
             (text (buffer-substring-no-properties (point-min) (point-max)))
             (saved-point (point)))
        (setq bookkeeping-us
              (+ bookkeeping-us
                 (max 0 (round (* 1000000 (- (float-time) book-started))))))
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
          ("process-output"
           ;; Reading a subprocess's output: spawn, read, DECODE, insert.
           ;;
           ;; Every compilation, grep, and language-server session pays this
           ;; path, and no other row on the board touches it.  `call-process`
           ;; rather than an async filter because a benchmark has to be
           ;; deterministic -- it reaches the same decoder
           ;; (`decode_process_run_in_context`), which is what this row exists
           ;; to hold honest; it does not cover filter dispatch or partial-run
           ;; carryover, and a row that claimed to would be lying.
           (let ((path (neomacs-perf-workload--required-environment
                        "NEOMACS_PERF_SOURCE")))
             (setq type-us
                   (+ type-us
                      (neomacs-perf-workload--time
                       (lambda ()
                         (with-temp-buffer
                           (call-process "cat" nil t nil path))))))))
          ("file-open"
           ;; Opening a file, split into the two halves that behave
           ;; differently, because a single number hides both.
           ;;
           ;; `insert-file-contents' is decode + buffer insert, and it is on
           ;; the path of EVERY file a session opens; no other row on the
           ;; board times it (large-file-editing loads its buffer before the
           ;; sampling window opens).  Fontification is the other half and is
           ;; large enough to bury it, so the two are timed apart.
           (let ((path (neomacs-perf-workload--required-environment
                        "NEOMACS_PERF_SOURCE")))
             (with-temp-buffer
               (setq type-us
                     (+ type-us
                        (neomacs-perf-workload--time
                         (lambda () (insert-file-contents path)))))
               (setq fontify-us
                     (+ fontify-us
                        (neomacs-perf-workload--time
                         (lambda ()
                           (emacs-lisp-mode)
                           ;; ONE SCREENFUL, not the whole buffer.  Opening a
                           ;; file does not fontify it -- `jit-lock' fontifies
                           ;; what a window shows and defers the rest, so
                           ;; `font-lock-ensure' over 252 KB would measure
                           ;; something no `find-file' performs, and at ~95x
                           ;; the decode it would bury the half of this row
                           ;; that nothing else on the board covers.
                           (font-lock-ensure
                            (point-min)
                            (save-excursion
                              (goto-char (point-min))
                              (forward-line 60)
                              (point))))))))))
          ("sustained-editing"
           (setq type-us (+ type-us (neomacs-perf-workload--time
                                     #'neomacs-perf-workload--single-edit-cycle))))
          ("lsp-json-rpc"
           ;; One jsonrpc round trip, the eglot per-keystroke path: serialize a
           ;; request and parse a server reply. Timed together because that is
           ;; how a session pays for them.
           (setq regex-us
                 (+ regex-us
                    (neomacs-perf-workload--time
                     (lambda ()
                       (let ((payload (json-serialize
                                       neomacs-perf-workload--lsp-payload
                                       :null-object nil :false-object :json-false)))
                         (json-parse-string payload :object-type 'plist
                                            :null-object nil
                                            :false-object :json-false)))))))
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
          ((or "org-editing" "org-editing-heavy")
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
        (let ((book-started (float-time)))
          (neomacs-perf-workload--restore text saved-point)
          (setq bookkeeping-us
                (+ bookkeeping-us
                   (max 0 (round (* 1000000 (- (float-time) book-started)))))))))
    `((bookkeeping . ,bookkeeping-us)
      (type . ,type-us)
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

(defun neomacs-perf-workload--lsp-message (count)
  "Build one `textDocument/publishDiagnostics' plist with COUNT diagnostics.

Shaped like what a language server actually sends -- nested ranges, a
message string per item, related information -- because payload SIZE and
SHAPE are what this workload exists to hold fixed.  A 1 KB sample measures
neither engine's JSON path: both complete it in tens of microseconds."
  (list :jsonrpc "2.0"
        :method "textDocument/publishDiagnostics"
        :params
        (list :uri "file:///tmp/neomacs-perf/main.cpp"
              :diagnostics
              (vconcat
               (let (items)
                 (dotimes (i count)
                   (push (list :range (list :start (list :line i :character 4)
                                            :end (list :line i :character 32))
                               :severity 1
                               :code "no_member"
                               :source "clang"
                               :message (format "no member named 'field_%d' in 'Widget'" i)
                               :relatedInformation
                               (vector (list :location
                                             (list :uri "file:///tmp/neomacs-perf/widget.h"
                                                   :range (list :start (list :line i :character 0)
                                                                :end (list :line i :character 9)))
                                             :message "declared here")))
                         items))
                 (nreverse items))))))

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
        ;; What the harness spent on its own snapshot/restore inside the
        ;; timed window, so a reader can tell the workload from the scaffold.
        (harness_bookkeeping_us . ,(alist-get 'bookkeeping phases))
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

(defun neomacs-perf-workload--maybe-write-latency-trace ()
  "Write the per-keystroke latency decomposition, when one was collected.

One space-separated record per sample, in the order the keystrokes ran,
so a slow sample can be lined up against the frames around it."
  (let ((path (getenv "NEOMACS_PERF_LATENCY_TRACE_FILE")))
    (when (and path neomacs-perf-workload--latency-trace)
      (with-temp-file path
        (insert "wall_us cpu_us gcs gc_us start_us\n")
        (dolist (record (nreverse neomacs-perf-workload--latency-trace))
          (insert (format "%d %d %d %d %d\n" (nth 0 record) (nth 1 record)
                          (nth 2 record) (nth 3 record) (nth 4 record))))))))

(defun neomacs-perf-workload--maybe-release-startup-gc-ceiling ()
  "Lift Neomacs' startup GC ceiling before measuring, when asked.

Neomacs caps its GC allocation interval at 4 MB while
`neomacs--startup-gc-ceiling-active' is set, and releases it 30 seconds
after startup settles.  Every workload here finishes well inside that
window -- gui-input-latency takes about 1.2 seconds -- so by default these
rows measure the STARTUP collection policy rather than the steady-state
one a user's session runs under.  Setting
NEOMACS_PERF_RELEASE_STARTUP_GC_CEILING=1 measures the other side of that.
GNU has no such variable and ignores this."
  (when (and (equal (getenv "NEOMACS_PERF_RELEASE_STARTUP_GC_CEILING") "1")
             (boundp 'neomacs--startup-gc-ceiling-active))
    (setq neomacs--startup-gc-ceiling-active nil)))

(defun neomacs-perf-workload--run ()
  (neomacs-perf-workload--maybe-release-startup-gc-ceiling)
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
         (phases '((bookkeeping . 0) (type . 0) (comment . 0) (kill-yank . 0)
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
    (neomacs-perf-workload--maybe-write-latency-trace)
    (write-region "done\n" nil sentinel-path nil 'silent)
    (kill-emacs exit-code)))

(if noninteractive
    (neomacs-perf-workload--run)
  (run-at-time 0 nil #'neomacs-perf-workload--run))

;;; editor-workloads.el ends here
