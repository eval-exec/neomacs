//! Practical parity for Helm Git Grep's public search workflows.
//!
//! The cases drive the package against an owned real Git repository, exercise
//! public Helm entry points and actions through a narrow UI seam, and pin the
//! exact Git process boundary, transformed candidates, navigation, options,
//! saved results, failure recovery, and cleanup.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_GIT_GREP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'helm-git-grep)

(defconst hgg394-test-source-sha256
  "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc")
(defconst hgg394-test-git-sha256
  "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352")
(defconst hgg394-test-main-js
  "export function deployCafé() {\n  return \"Deploy界\";\n}\n")
(defconst hgg394-test-other-js
  "export const fallback = \"deploy界 fallback\";\n")
(defconst hgg394-test-test-js
  "test(\"Deploy界\", () => true);\n")
(defconst hgg394-test-readme
  "# Deploy界 runbook\n")

(defconst hgg394-test-real-process-file (symbol-function 'process-file))
(defconst hgg394-test-real-call-process (symbol-function 'call-process))
(defconst hgg394-test-real-start-process (symbol-function 'start-process))
(defconst hgg394-test-real-make-process (symbol-function 'make-process))

(defvar hgg394-test-boundary-state nil)
(defvar hgg394-test-owned-processes nil)
(defvar hgg394-test-process-output-buffer nil)
(defvar hgg394-test-approved-start-depth 0)
(defvar hgg394-test-approved-process-file-context nil)
(defvar hgg394-test-stage nil)
(defvar hgg394-test-root nil)

(defun hgg394-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defconst hgg394-test-git-executable
  (let ((file (executable-find "git")))
    (unless (and file
                 (file-regular-p file)
                 (equal (hgg394-test-file-sha256 file)
                        hgg394-test-git-sha256))
      (error "Unexpected Git executable: %S" file))
    (file-truename file)))

(let ((version
       (with-temp-buffer
         (unless (zerop (funcall hgg394-test-real-process-file
                                 hgg394-test-git-executable nil t nil
                                 "--version"))
           (error "Unable to query Git version"))
         (string-trim (buffer-string)))))
  (unless (equal version "git version 2.51.2")
    (error "Unexpected Git version: %S" version)))

(let ((file (symbol-file 'helm-git-grep 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file) "helm-git-grep.el")
               (equal (hgg394-test-file-sha256 file)
                      hgg394-test-source-sha256))
    (error "Unexpected installed Helm Git Grep source: %S" file)))

(defun hgg394-test-condition (condition)
  (list :type (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun hgg394-test-snapshot (value)
  (cond ((stringp value) (copy-sequence value))
        ((consp value)
         (cons (hgg394-test-snapshot (car value))
               (hgg394-test-snapshot (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'hgg394-test-snapshot value)))
        (t value)))

(defun hgg394-test-write (file text)
  (make-directory (file-name-directory file) t)
  (with-temp-file file (insert text)))

(defun hgg394-test-owned-buffer (name)
  (generate-new-buffer name))

(defun hgg394-test-park-buffer (name suffix)
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((old-name (buffer-name)))
        (rename-buffer (format " %s-%s" suffix (sxhash-eq buffer)) t)
        (cons buffer old-name)))))

(defun hgg394-test-window-state ()
  (list
   :selected (selected-window)
   :windows
   (mapcar
    (lambda (window)
      (list :window window :buffer (window-buffer window)
            :point (window-point window)
            :start (window-start window) :hscroll (window-hscroll window)
            :vscroll (window-vscroll window t)
            :prev (copy-tree (window-prev-buffers window))
            :next (copy-tree (window-next-buffers window))))
    (window-list nil 'no-minibuf))))

(defun hgg394-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry (plist-get state :windows))
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Baseline Helm Git Grep window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun hgg394-test-git-setup (root)
  (hgg394-test-write (expand-file-name "src/main.js" root)
                     hgg394-test-main-js)
  (hgg394-test-write (expand-file-name "src/other.js" root)
                     hgg394-test-other-js)
  (hgg394-test-write (expand-file-name "test/main.test.js" root)
                     hgg394-test-test-js)
  (hgg394-test-write (expand-file-name "README.md" root)
                     hgg394-test-readme)
  (let ((default-directory root))
    (dolist (args '(("init" "-q")
                    ("add" "--" "README.md" "src/main.js" "src/other.js"
                           "test/main.test.js")))
      (unless (zerop (apply hgg394-test-real-process-file
                            hgg394-test-git-executable nil nil nil args))
        (error "Failed to build Git fixture: %S" args)))))

(defun hgg394-test-fixture-state (root)
  (mapcar
   (lambda (relative)
     (let ((file (expand-file-name relative root)))
       (list relative (hgg394-test-file-sha256 file))))
   '("README.md" "src/main.js" "src/other.js" "test/main.test.js")))

(defun hgg394-test-relative-directory ()
  (file-relative-name (file-name-as-directory (file-truename default-directory))
                      hgg394-test-root))

(defun hgg394-test-expect-boundary (kind program args)
  (let* ((state (default-value 'hgg394-test-boundary-state))
         (remaining (and state (aref state 0))))
  (unless remaining
    (error "Unexpected %s boundary: %S" kind (cons program args)))
  (let* ((plan (car remaining))
         (actual (list :kind kind
                       :cwd (hgg394-test-relative-directory)
                       :program (copy-sequence program)
                       :args (mapcar #'copy-sequence args)
                       :environment
                       (mapcar (lambda (name)
                                 (cons name (copy-sequence (getenv name))))
                               '("GIT_CONFIG_NOSYSTEM" "GIT_CONFIG_GLOBAL"
                                 "GIT_OPTIONAL_LOCKS" "LANG" "LC_ALL"))))
         (expected (list :kind (plist-get plan :kind)
                         :cwd (plist-get plan :cwd)
                         :program "git"
                         :args (plist-get plan :args)
                         :environment
                         '(("GIT_CONFIG_NOSYSTEM" . "1")
                           ("GIT_CONFIG_GLOBAL" . "/dev/null")
                           ("GIT_OPTIONAL_LOCKS" . "0")
                           ("LANG" . "C.UTF-8")
                           ("LC_ALL" . "C.UTF-8")))))
    (unless (equal actual expected)
      (error "Git boundary mismatch at %S: expected %S, got %S"
             hgg394-test-stage expected actual))
    (aset state 0 (cdr remaining))
    (aset state 1 (append (aref state 1) (list actual)))
    plan)))

(defun hgg394-test-process-file
    (program infile destination display &rest args)
  (let ((approved (and hgg394-test-approved-process-file-context
                       (aref hgg394-test-approved-process-file-context 0))))
    (if approved
      (let ((actual (list program infile destination display args)))
        (unless (equal actual approved)
          (error "Unexpected nested process-file call: %S" actual))
        (apply hgg394-test-real-process-file
               program infile destination display args))
      (unless (and (equal program "git")
                   (null infile) (equal destination '(t nil)) (null display))
        (error "Unexpected process-file shape: %S"
               (list program infile destination display args)))
      (hgg394-test-expect-boundary 'process-file program args)
      (let ((call (list hgg394-test-git-executable
                        infile destination display args)))
        (aset hgg394-test-approved-process-file-context 0 call)
        (unwind-protect
            (apply hgg394-test-real-process-file
                   hgg394-test-git-executable infile destination display args)
          (aset hgg394-test-approved-process-file-context 0 nil))))))

(defun hgg394-test-call-process
    (program infile destination display &rest args)
  (if (and hgg394-test-approved-process-file-context
           (aref hgg394-test-approved-process-file-context 0))
      (progn
        (unless (equal program hgg394-test-git-executable)
          (error "Unexpected process-file descent: %S" program))
        (apply hgg394-test-real-call-process
               program infile destination display args))
    (unless (and (equal program "git") (null infile) (bufferp destination)
                 (null display))
      (error "Unexpected call-process shape: %S"
             (list program infile destination display args)))
    (hgg394-test-expect-boundary 'call-process program args)
    (apply hgg394-test-real-call-process
           hgg394-test-git-executable infile destination display args)))

(defun hgg394-test-note-sentinel (process &rest _)
  "Record on PROCESS that its sentinel has run.
That is the causal end of the child's output, and `process-live-p' going
nil is not.  GNU reaps the child in `handle_child_signal', which sets
`raw_status_new' (src/process.c:7748) -- enough for `process-status' to
answer `exit' (src/process.c:1188-1189) -- and in the same pass calls
`delete_read_fd' (src/process.c:7760), so ordinary reading of the pipe has
STOPPED at exactly the moment `process-live-p' goes nil.  Whatever the
child had already written is recovered only by the drain loop inside
`status_notify' (src/process.c:7896-7911), which runs immediately before
`exec_sentinel' (src/process.c:7937).  So the sentinel having run is a fact
about the output; the process being dead is a fact about the clock."
  (process-put process 'hgg394-test-sentinel-ran t))

(defun hgg394-test-start-process (name buffer program &rest args)
  (unless (and (equal name "git-grep-process") (null buffer)
               (equal program "git")
               (buffer-live-p hgg394-test-process-output-buffer))
    (error "Unexpected start-process shape: %S"
           (list name buffer program args hgg394-test-process-output-buffer)))
  (hgg394-test-expect-boundary 'start-process program args)
  (let ((hgg394-test-approved-start-depth
         (1+ hgg394-test-approved-start-depth)))
    (let ((process
           (apply hgg394-test-real-start-process
                  name hgg394-test-process-output-buffer
                  hgg394-test-git-executable args)))
      (unless (and (processp process)
                   (eq (process-buffer process)
                       hgg394-test-process-output-buffer)
                   (equal (process-command process)
                          (cons hgg394-test-git-executable args)))
        (when (processp process) (delete-process process))
        (error "Unexpected created Git process: %S" process))
      (push process hgg394-test-owned-processes)
      (set-process-sentinel process #'ignore)
      ;; Attach the completion witness here, at creation, because this is the
      ;; only moment that is guaranteed to precede the sentinel: Emacs runs
      ;; process sentinels only from `status_notify', which runs from the event
      ;; loop, so nothing can have fired before the first
      ;; `accept-process-output'.
      (add-function :after (process-sentinel process)
                    #'hgg394-test-note-sentinel)
      process)))

(defun hgg394-test-make-process (&rest args)
  (unless (> hgg394-test-approved-start-depth 0)
    (error "Unexpected direct make-process: %S" args))
  (apply hgg394-test-real-make-process args))

(defun hgg394-test-wait-process (process)
  "Wait until PROCESS has run its sentinel, then report how it exited.
The caller reads `hgg394-test-process-output-buffer' the moment this
returns, so what it needs is the fact that the child's output has all been
read -- not the fact that the child is dead.  Those are different moments
and they are not in the order one would guess: see
`hgg394-test-note-sentinel'.  Waiting for the sentinel removes the choice;
signalling rather than returning means a future edit that goes back to
waiting on the clock fails on its first run instead of moving a snapshot
months later."
  (let ((deadline (+ (float-time) 10.0)))
    (while (and (not (process-get process 'hgg394-test-sentinel-ran))
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (unless (process-get process 'hgg394-test-sentinel-ran)
      (error "hgg394-test-wait-process: %S never ran its sentinel; \
`%s' holds only as much of the child's output as had been read"
             process (buffer-name hgg394-test-process-output-buffer)))
    (unless (and (eq (process-status process) 'exit)
                 (zerop (process-exit-status process)))
      (error "Git grep process failed: %S/%S"
             (process-status process) (process-exit-status process)))
    (list :status (process-status process)
          :exit (process-exit-status process)
          :stable t)))

(defun hgg394-test-face-runs (string)
  (let ((position 0) runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (or (next-single-property-change
                        position 'face string (length string))
                       (length string))))
        (when face
          (push (list :range (list position next)
                      :text (substring-no-properties string position next)
                      :face face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun hgg394-test-candidate-state (candidate root)
  (let ((display (car candidate))
        (real (cdr candidate)))
    (list :display (substring-no-properties display)
          :faces (hgg394-test-face-runs display)
          :line (nth 0 real)
          :content (substring-no-properties (nth 1 real))
          :file (file-relative-name (nth 2 real) root))))

(defun hgg394-test-location (root)
  (let ((buffer (window-buffer (selected-window))))
    (with-current-buffer buffer
      (list :file (and buffer-file-name
                       (file-relative-name buffer-file-name root))
            :point (window-point (selected-window))
            :line (line-number-at-pos (window-point (selected-window)))
            :column (save-excursion
                      (goto-char (window-point (selected-window)))
                      (current-column))
            :text (save-excursion
                    (goto-char (window-point (selected-window)))
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))))))

(defun hgg394-test-resolve-source (source)
  (if (symbolp source) (symbol-value source) source))

(defun hgg394-test-resolve-actions (source)
  (let ((actions (helm-attr 'action source)))
    (if (symbolp actions) (symbol-value actions) actions)))

(defun hgg394-test-helm-dispatch (root plan arguments)
  (let* ((query (plist-get plan :query))
         (expected-input (plist-get plan :input))
         (action-name (plist-get plan :action))
         (sources (mapcar #'hgg394-test-resolve-source
                          (plist-get arguments :sources)))
         (helm-pattern query)
         (helm-input query)
         source-states selected process-state)
    (unless (and (equal (plist-get arguments :buffer) "*helm git grep*")
                 (equal (plist-get arguments :input) expected-input)
                 (eq (plist-get arguments :keymap) helm-git-grep-map)
                 (equal (plist-get arguments :candidate-number-limit)
                        helm-git-grep-candidate-number-limit))
      (error "Unexpected Helm invocation: %S" arguments))
    (dolist (source sources)
      (let* ((helm-current-source source)
             (init (helm-attr 'init source))
             (process-function (helm-attr 'candidates-process source))
             (hgg394-test-process-output-buffer
              (hgg394-test-owned-buffer " *hgg394-git-output*"))
             raw transformed pairs)
        (when init
          (let ((hgg394-test-stage 'source-init))
            (helm-apply-functions-from-source source init)))
        (let* ((hgg394-test-stage 'candidate-process)
               (process (funcall process-function)))
          (cond ((processp process)
                 (setq process-state (hgg394-test-wait-process process)
                       raw (with-current-buffer hgg394-test-process-output-buffer
                             (buffer-string))))
                ((null process) (setq raw ""))
                (t (error "Unexpected candidate process result: %S" process))))
        (setq transformed
              (helm-apply-functions-from-source
               source (helm-attr 'filtered-candidate-transformer source)
               (split-string raw "\n" t) source)
              pairs transformed)
        (push (list :name (helm-attr 'name source)
                    :base (let ((base (helm-attr 'base-directory source)))
                            (and base (file-relative-name base root)))
                    :raw (copy-sequence raw)
                    :process process-state
                    :candidates
                    (mapcar (lambda (candidate)
                              (hgg394-test-candidate-state candidate root))
                            pairs))
              source-states)
        (when (and action-name pairs (null selected))
          (let* ((action (cdr (assoc action-name
                                     (hgg394-test-resolve-actions source))))
                 (candidate (nth (or (plist-get plan :candidate) 0) pairs))
                 (helm-buffer (hgg394-test-owned-buffer " *hgg394-helm*"))
                 (helm-current-source source)
                 (helm-in-persistent-action nil))
            (unless (and candidate (functionp action))
              (error "Missing Helm candidate/action: %S/%S" candidate action-name))
            (with-current-buffer helm-buffer
              (insert "Candidates:\n")
              (dolist (pair pairs) (insert (car pair) "\n"))
              (goto-char (point-min))
              (forward-line (1+ (or (plist-get plan :candidate) 0))))
            (setq selected (hgg394-test-candidate-state candidate root))
            (let ((hgg394-test-stage 'action))
              (funcall action (cdr candidate)))))))
    (list :input (plist-get arguments :input)
          :query query
          :sources (nreverse source-states)
          :action action-name
          :selected selected
          :history (copy-sequence helm-git-grep-history)
          :location (and (equal action-name "Find File")
                         (hgg394-test-location root)))))

(defun hgg394-test-run-public-helm (root thunk plans)
  (let ((remaining (copy-tree plans)) calls)
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest arguments)
                 (unless remaining
                   (error "Unexpected public Helm invocation: %S" arguments))
                 (push (hgg394-test-helm-dispatch
                        root (pop remaining) arguments)
                       calls))))
      (funcall thunk))
    (when remaining (error "Missing public Helm invocations: %S" remaining))
    (nreverse calls)))

(defun hgg394-test-run-reruns (thunk)
  (let (calls)
    (cl-letf (((symbol-function 'helm-run-after-exit)
               (lambda (function) (funcall function)))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (push (list :input (plist-get arguments :input)
                             :buffer (plist-get arguments :buffer)
                             :sources (copy-sequence
                                       (plist-get arguments :sources))
                             :keymap (eq (plist-get arguments :keymap)
                                         helm-git-grep-map)
                             :limit (plist-get arguments
                                               :candidate-number-limit)
                             :options
                             (list helm-git-grep-ignore-case
                                   helm-git-grep-wordgrep
                                   helm-git-grep-showing-leading-and-trailing-lines
                                   helm-git-grep-base-directory
                                   helm-git-grep-pathspec-available)
                             :args (helm-git-grep-args))
                       calls))))
      (funcall thunk))
    (nreverse calls)))

(defun hgg394-test-run (plans body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "helm-git-grep/" sandbox))))
         (outside-root (and sandbox
                            (file-name-as-directory
                             (expand-file-name "helm-git-grep-outside/"
                                               sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (hgg394-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (parked nil)
         (source-base-before (helm-attr 'base-directory helm-git-grep-source))
         (next-error-last-buffer next-error-last-buffer)
         (print-circle nil)
         (process-environment (copy-sequence process-environment))
         (helm--local-variables nil)
         (helm-git-grep-sources '(helm-git-grep-source))
         (helm-git-grep-candidate-number-limit 300)
         (helm-git-grep-history nil)
         (helm-git-grep-max-length-history 100)
         (helm-git-grep-ignore-case t)
         (helm-git-grep-wordgrep nil)
         (helm-git-grep-showing-leading-and-trailing-lines nil)
         (helm-git-grep-showing-leading-and-trailing-lines-number 1)
         (helm-git-grep-at-point-deactivate-mark nil)
         (helm-git-grep-base-directory 'root)
         (helm-git-grep-pathspecs nil)
         (helm-git-grep-pathspec-available t)
         (hgg394-test-root root)
         (boundary-state-before (default-value 'hgg394-test-boundary-state))
         (boundary-state (vector (copy-tree plans) nil))
         (expected-boundary-count (length plans))
         (hgg394-test-owned-processes nil)
         (hgg394-test-approved-start-depth 0)
         (hgg394-test-approved-process-file-context (vector nil))
         (hgg394-test-stage nil)
         result body-error cleanup-errors fixture-before fixture-after)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root outside-root
                           (file-name-absolute-p root)
                           (file-name-absolute-p outside-root))
                (error "Missing absolute Helm Git Grep sandbox roots"))
              (dolist (directory (list root outside-root))
                (when (file-exists-p directory)
                  (error "Helm Git Grep sandbox root already exists: %s"
                         directory)))
              (dolist (buffer-spec
                       '(("*helm git grep*" . "hgg394-helm")
                         ("*hggrep*" . "hgg394-results")
                         ("*helm-git-grep ls-files*" . "hgg394-ls")))
                (when-let* ((entry (hgg394-test-park-buffer
                                    (car buffer-spec) (cdr buffer-spec))))
                  (push entry parked)))
              (setq parked (nreverse parked))
              (setenv "GIT_CONFIG_NOSYSTEM" "1")
              (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
              (setenv "GIT_OPTIONAL_LOCKS" "0")
              (setenv "LANG" "C.UTF-8")
              (setenv "LC_ALL" "C.UTF-8")
              (make-directory root t)
              (hgg394-test-git-setup root)
              (setq fixture-before (hgg394-test-fixture-state root))
              (set-default 'hgg394-test-boundary-state boundary-state)
              (condition-case condition
                  (cl-letf
                      (((symbol-function 'process-file)
                        #'hgg394-test-process-file)
                       ((symbol-function 'call-process)
                        #'hgg394-test-call-process)
                       ((symbol-function 'start-process)
                        #'hgg394-test-start-process)
                       ((symbol-function 'make-process)
                        #'hgg394-test-make-process)
                       ((symbol-function 'make-network-process)
                        (lambda (&rest arguments)
                          (error "Unexpected network process: %S" arguments)))
                       ((symbol-function 'url-retrieve)
                        (lambda (&rest arguments)
                          (error "Unexpected URL retrieval: %S" arguments)))
                       ((symbol-function 'helm)
                        (lambda (&rest arguments)
                          (error "Unplanned Helm invocation: %S" arguments))))
                    (save-window-excursion
                      (save-current-buffer
                        (setq result (funcall body root)))))
                (t (setq body-error (hgg394-test-condition condition))))
              (condition-case condition
                  (setq fixture-after (hgg394-test-fixture-state root))
                (t (push (list :fixture-verification
                               (hgg394-test-condition condition))
                         cleanup-errors))))
          (t (setq body-error (hgg394-test-condition condition))))
      (condition-case condition
          (set-default 'hgg394-test-boundary-state boundary-state-before)
        (t (push (hgg394-test-condition condition) cleanup-errors)))
      (when (aref boundary-state 0)
        (push (list :missing-boundaries (copy-tree (aref boundary-state 0)))
              cleanup-errors))
      (unless (= (length (aref boundary-state 1))
                 expected-boundary-count)
        (push (list :boundary-count
                    :expected expected-boundary-count
                    :actual (length (aref boundary-state 1)))
              cleanup-errors))
      (condition-case condition
          (hgg394-test-restore-windows window-before window-state-before)
        (t (push (hgg394-test-condition condition) cleanup-errors)))
      (dolist (process hgg394-test-owned-processes)
        (condition-case condition
            (when (processp process) (delete-process process))
          (t (push (hgg394-test-condition condition) cleanup-errors))))
      (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer))))
          (t (push (hgg394-test-condition condition) cleanup-errors))))
      (dolist (timer (seq-difference timer-list timers-before #'eq))
        (condition-case condition (cancel-timer timer)
          (t (push (hgg394-test-condition condition) cleanup-errors))))
      (dolist (frame (seq-difference (frame-list) frames-before #'eq))
        (condition-case condition (delete-frame frame t)
          (t (push (hgg394-test-condition condition) cleanup-errors))))
      (condition-case condition
          (helm-attrset 'base-directory source-base-before helm-git-grep-source)
        (t (push (hgg394-test-condition condition) cleanup-errors)))
      (condition-case condition
          (hgg394-test-restore-windows window-before window-state-before)
        (t (push (hgg394-test-condition condition) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry)
                  (rename-buffer (cdr entry) t))
              (error "Parked buffer died: %S" (cdr entry)))
          (t (push (hgg394-test-condition condition) cleanup-errors))))
      (condition-case condition
          (when (buffer-live-p buffer-before) (set-buffer buffer-before))
        (t (push (hgg394-test-condition condition) cleanup-errors)))
      (dolist (directory (list root outside-root))
        (condition-case condition
            (when (and directory (file-exists-p directory))
              (delete-directory directory t))
          (t (push (hgg394-test-condition condition) cleanup-errors)))))
    (let ((cleanup
           (list :fixture-unchanged (equal fixture-before fixture-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list)
                                                     buffers-before #'eq)))
                 :new-processes (length (seq-difference
                                         (process-list) processes-before #'eq))
                 :new-timers (length (seq-difference timer-list timers-before #'eq))
                 :new-frames (length (seq-difference (frame-list) frames-before #'eq))
                 :root-exists (and root (file-exists-p root))
                 :outside-root-exists
                 (and outside-root (file-exists-p outside-root))
                 :source-base-restored
                 (equal (helm-attr 'base-directory helm-git-grep-source)
                        source-base-before)
                 :boundary-state-restored
                 (eq (default-value 'hgg394-test-boundary-state)
                     boundary-state-before)
                 :window-restored
                 (equal (hgg394-test-window-state) window-state-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Helm Git Grep workflow failed: %S" (list result cleanup))
        (hgg394-test-snapshot
         (list :provenance
               (list :source hgg394-test-source-sha256
                     :git-version "git version 2.51.2"
                     :git-sha hgg394-test-git-sha256
                     :fixture fixture-before)
               :result result
               :boundaries (aref boundary-state 1)
               :cleanup cleanup))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_GIT_GREP_MELPA_PIN, "helm-git-grep.el")
        .expect("prepare exact shallow Helm Git Grep source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_at_point_search_runs_real_git_and_navigates() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_at_point_search_runs_real_git_and_navigates",
        r####"
(hgg394-test-run
 '((:kind process-file :cwd "src/"
    :args ("--no-pager" "rev-parse" "--show-cdup"))
   (:kind start-process :cwd "./"
    :args ("--no-pager" "grep" "--null" "-n" "--no-color"
           "-e" "Deploy界" "--" "src/*.js" ":!:src/other.js")))
 (lambda (root)
   (let* ((file (expand-file-name "src/main.js" root))
          (buffer (find-file-noselect file))
          (helm-git-grep-ignore-case nil)
          (helm-git-grep-pathspecs '("src/*.js" ":!:src/other.js")))
     (with-current-buffer buffer
       (goto-char (point-min))
       (search-forward "Deploy界")
       (backward-char)
       (hgg394-test-run-public-helm
        root
        (lambda () (call-interactively #'helm-git-grep-at-point))
        '((:input "Deploy界 " :query "Deploy界" :action "Find File")))))))
"####,
        expect![[
            r#"OK (:provenance (:source "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc" :git-version "git version 2.51.2" :git-sha "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352" :fixture (("README.md" "46300e5fe8eb4b4662be23293d39e88631b0f7eb45f842f0ecb776effddd56c6") ("src/main.js" "3a9d558ceb5e8705b70b5124c78a9a8eb4a7df219039a7aa987da16a707acb9d") ("src/other.js" "8506d195735f8ee0d9bd98ae3f1f2f953285bcb8dd67e7a69582442cde55770f") ("test/main.test.js" "4fc06e96e400a94d2141e7ce5a076b372435e4eaed339ac6e9a52cd2d4d1b42f"))) :result ((:input "Deploy界 " :query "Deploy界" :sources ((:name "Git Grep" :base "./" :raw "src/main.js\0002\0  return \"Deploy界\";\n" :process (:status exit :exit 0 :stable t) :candidates ((:display "src/main.js:2:  return \"Deploy界\";" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "2" :face helm-git-grep-line) (:range (24 31) :text "Deploy界" :face (helm-match helm-git-grep-match))) :line 2 :content "  return \"Deploy界\";" :file "src/main.js")))) :action "Find File" :selected (:display "src/main.js:2:  return \"Deploy界\";" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "2" :face helm-git-grep-line) (:range (24 31) :text "Deploy界" :face (helm-match helm-git-grep-match))) :line 2 :content "  return \"Deploy界\";" :file "src/main.js") :history ("Deploy界") :location (:file "src/main.js" :point 32 :line 2 :column 0 :text "  return \"Deploy界\";"))) :boundaries ((:kind process-file :cwd "src/" :program "git" :args ("--no-pager" "rev-parse" "--show-cdup") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8"))) (:kind start-process :cwd "./" :program "git" :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-e" "Deploy界" "--" "src/*.js" ":!:src/other.js") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8")))) :cleanup (:fixture-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :outside-root-exists nil :source-base-restored t :boundary-state-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn documented_option_toggles_rerun_the_public_search() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_option_toggles_rerun_the_public_search",
        r####"
(hgg394-test-run
 nil
 (lambda (root)
   (let ((buffer (hgg394-test-owned-buffer " *hgg394-toggle*")))
     (with-current-buffer buffer
       (let ((default-directory root)
             (helm-input "deploy fallback")
             (helm-pattern "deploy fallback")
             (helm-git-grep-pathspecs '("src/*.js")))
         (list
          :reruns
          (hgg394-test-run-reruns
           (lambda ()
             (call-interactively #'helm-git-grep-toggle-ignore-case)
             (call-interactively #'helm-git-grep-toggle-wordgrep)
             (call-interactively
              #'helm-git-grep-toggle-showing-trailing-leading-line)
             (call-interactively #'helm-git-grep-toggle-base-directory)
             (call-interactively #'helm-git-grep-pathspec-toggle-availability)))
          :state
          (list :ignore-case helm-git-grep-ignore-case
                :word helm-git-grep-wordgrep
                :context helm-git-grep-showing-leading-and-trailing-lines
                :base helm-git-grep-base-directory
                :pathspec-available helm-git-grep-pathspec-available
                :args (helm-git-grep-args))))))))
"####,
        expect![[
            r#"OK (:provenance (:source "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc" :git-version "git version 2.51.2" :git-sha "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352" :fixture (("README.md" "46300e5fe8eb4b4662be23293d39e88631b0f7eb45f842f0ecb776effddd56c6") ("src/main.js" "3a9d558ceb5e8705b70b5124c78a9a8eb4a7df219039a7aa987da16a707acb9d") ("src/other.js" "8506d195735f8ee0d9bd98ae3f1f2f953285bcb8dd67e7a69582442cde55770f") ("test/main.test.js" "4fc06e96e400a94d2141e7ce5a076b372435e4eaed339ac6e9a52cd2d4d1b42f"))) :result (:reruns ((:input "deploy fallback" :buffer "*helm git grep*" :sources (helm-git-grep-source) :keymap t :limit 300 :options (nil nil nil root t) :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-e" "deploy" "--and" "-e" "fallback" "--" "src/*.js")) (:input "deploy fallback" :buffer "*helm git grep*" :sources (helm-git-grep-source) :keymap t :limit 300 :options (nil t nil root t) :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-w" "-e" "deploy" "--and" "-e" "fallback" "--" "src/*.js")) (:input "deploy fallback" :buffer "*helm git grep*" :sources (helm-git-grep-source) :keymap t :limit 300 :options (nil t t root t) :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-w" "-1" "-e" "deploy" "--and" "-e" "fallback" "--" "src/*.js")) (:input "deploy fallback" :buffer "*helm git grep*" :sources (helm-git-grep-source) :keymap t :limit 300 :options (nil t t current t) :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-w" "-1" "-e" "deploy" "--and" "-e" "fallback" "--" "src/*.js")) (:input "deploy fallback" :buffer "*helm git grep*" :sources (helm-git-grep-source) :keymap t :limit 300 :options (nil t t current nil) :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-w" "-1" "-e" "deploy" "--and" "-e" "fallback"))) :state (:ignore-case nil :word t :context t :base current :pathspec-available nil :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-w" "-1" "-e" "deploy" "--and" "-e" "fallback"))) :boundaries nil :cleanup (:fixture-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :outside-root-exists nil :source-base-restored t :boundary-state-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_pathspec_listing_runs_real_git_and_displays_owned_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_pathspec_listing_runs_real_git_and_displays_owned_files",
        r####"
(hgg394-test-run
 '((:kind call-process :cwd "./"
    :args ("ls-files" "--" "src/*.js" ":!:src/other.js")))
 (lambda (root)
   (let ((caller (hgg394-test-owned-buffer " *hgg394-ls-caller*"))
         (helm-git-grep-pathspecs '("src/*.js" ":!:src/other.js")))
     (with-current-buffer caller
       (let ((default-directory root))
         (call-interactively #'helm-git-grep-ls-files-limited-by-pathspec)))
     (let ((output (get-buffer "*helm-git-grep ls-files*")))
       (with-current-buffer output
         (list :displayed (and (get-buffer-window output t) t)
               :mode major-mode
               :read-only buffer-read-only
               :default-directory
               (file-relative-name default-directory root)
               :text (buffer-substring-no-properties
                      (point-min) (point-max))))))))
"####,
        expect![[
            r#"OK (:provenance (:source "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc" :git-version "git version 2.51.2" :git-sha "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352" :fixture (("README.md" "46300e5fe8eb4b4662be23293d39e88631b0f7eb45f842f0ecb776effddd56c6") ("src/main.js" "3a9d558ceb5e8705b70b5124c78a9a8eb4a7df219039a7aa987da16a707acb9d") ("src/other.js" "8506d195735f8ee0d9bd98ae3f1f2f953285bcb8dd67e7a69582442cde55770f") ("test/main.test.js" "4fc06e96e400a94d2141e7ce5a076b372435e4eaed339ac6e9a52cd2d4d1b42f"))) :result (:displayed t :mode fundamental-mode :read-only nil :default-directory "./" :text "git ls-files -- src/*.js :!:src/other.js\n\nsrc/main.js\n") :boundaries ((:kind call-process :cwd "./" :program "git" :args ("ls-files" "--" "src/*.js" ":!:src/other.js") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8")))) :cleanup (:fixture-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :outside-root-exists nil :source-base-restored t :boundary-state-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_save_results_builds_grep_buffer_and_next_error_navigates() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_save_results_builds_grep_buffer_and_next_error_navigates",
        r####"
(hgg394-test-run
 '((:kind process-file :cwd "./"
    :args ("--no-pager" "rev-parse" "--show-cdup"))
   (:kind start-process :cwd "./"
    :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-i"
           "-e" "deploy" "--" "src/*.js"))
   (:kind process-file :cwd "./"
    :args ("--no-pager" "rev-parse" "--show-cdup")))
 (lambda (root)
   (let ((default-directory root)
         (helm-git-grep-pathspecs '("src/*.js"))
         message search saved-state destination)
     (cl-letf (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (setq message (apply #'format format-string arguments)))))
       (setq search
             (hgg394-test-run-public-helm
              root #'helm-git-grep
              '((:input nil :query "deploy"
                 :action "Save results in grep buffer")))))
     (let ((buffer (get-buffer "*hggrep*")))
       (with-current-buffer buffer
         (setq saved-state
               (list :mode major-mode
                     :read-only buffer-read-only
                     :default-directory
                     (file-relative-name default-directory root)
                     :text
                     (replace-regexp-in-string
                      (regexp-quote root) "[ROOT]/"
                      (buffer-substring-no-properties
                       (point-min) (point-max)) t t)))
         (goto-char (point-min))
         (let ((next-error-last-buffer buffer))
           (next-error 1)))
       (setq destination (hgg394-test-location root)))
     (list :message message :search search
           :saved saved-state :destination destination))))
"####,
        expect![[
            r#"OK (:provenance (:source "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc" :git-version "git version 2.51.2" :git-sha "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352" :fixture (("README.md" "46300e5fe8eb4b4662be23293d39e88631b0f7eb45f842f0ecb776effddd56c6") ("src/main.js" "3a9d558ceb5e8705b70b5124c78a9a8eb4a7df219039a7aa987da16a707acb9d") ("src/other.js" "8506d195735f8ee0d9bd98ae3f1f2f953285bcb8dd67e7a69582442cde55770f") ("test/main.test.js" "4fc06e96e400a94d2141e7ce5a076b372435e4eaed339ac6e9a52cd2d4d1b42f"))) :result (:message "Helm Git Grep Results saved in `*hggrep*' buffer" :search ((:input nil :query "deploy" :sources ((:name "Git Grep" :base "./" :raw "src/main.js\0001\0export function deployCafé() {\nsrc/main.js\0002\0  return \"Deploy界\";\nsrc/other.js\0001\0export const fallback = \"deploy界 fallback\";\n" :process (:status exit :exit 0 :stable t) :candidates ((:display "src/main.js:1:export function deployCafé() {" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "1" :face helm-git-grep-line) (:range (30 36) :text "deploy" :face (helm-match helm-git-grep-match))) :line 1 :content "export function deployCafé() {" :file "src/main.js") (:display "src/main.js:2:  return \"Deploy界\";" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "2" :face helm-git-grep-line) (:range (24 30) :text "Deploy" :face (helm-match helm-git-grep-match))) :line 2 :content "  return \"Deploy界\";" :file "src/main.js") (:display "src/other.js:1:export const fallback = \"deploy界 fallback\";" :faces ((:range (0 12) :text "src/other.js" :face helm-git-grep-file) (:range (13 14) :text "1" :face helm-git-grep-line) (:range (40 46) :text "deploy" :face (helm-match helm-git-grep-match))) :line 1 :content "export const fallback = \"deploy界 fallback\";" :file "src/other.js")))) :action "Save results in grep buffer" :selected (:display "src/main.js:1:export function deployCafé() {" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "1" :face helm-git-grep-line) (:range (30 36) :text "deploy" :face (helm-match helm-git-grep-match))) :line 1 :content "export function deployCafé() {" :file "src/main.js") :history ("deploy") :location nil)) :saved (:mode helm-git-grep-mode :read-only t :default-directory "./" :text "-*- mode: grep; default-directory: \"[ROOT]/\" -*-\n\nGit Grep Results by: git --no-pager grep --null -n --no-color -i -e deploy -- src/*.js\n\nsrc/main.js:1:export function deployCafé() {\nsrc/main.js:2:  return \"Deploy界\";\nsrc/other.js:1:export const fallback = \"deploy界 fallback\";\n") :destination (:file "src/main.js" :point 1 :line 1 :column 0 :text "export function deployCafé() {")) :boundaries ((:kind process-file :cwd "./" :program "git" :args ("--no-pager" "rev-parse" "--show-cdup") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8"))) (:kind start-process :cwd "./" :program "git" :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-i" "-e" "deploy" "--" "src/*.js") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8"))) (:kind process-file :cwd "./" :program "git" :args ("--no-pager" "rev-parse" "--show-cdup") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8")))) :cleanup (:fixture-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :outside-root-exists nil :source-base-restored t :boundary-state-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn outside_repository_empty_search_recovers_in_owned_repository() -> ParityBatchCase {
    ParityBatchCase::value(
        "outside_repository_empty_search_recovers_in_owned_repository",
        r####"
(hgg394-test-run
 '((:kind process-file :cwd "../helm-git-grep-outside/"
    :args ("--no-pager" "rev-parse" "--show-cdup"))
   (:kind process-file :cwd "./"
    :args ("--no-pager" "rev-parse" "--show-cdup"))
   (:kind start-process :cwd "./"
    :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-i"
           "-e" "Deploy界")))
 (lambda (root)
   (let ((outside (expand-file-name "../helm-git-grep-outside/" root))
         missing recovery)
     (make-directory outside)
     (let ((default-directory outside))
       (setq missing
             (hgg394-test-run-public-helm
              root #'helm-git-grep
              '((:input nil :query "Deploy界")))))
     (let ((default-directory root))
       (setq recovery
             (hgg394-test-run-public-helm
              root #'helm-git-grep
              '((:input nil :query "Deploy界")))))
     (list :outside missing :recovery recovery))))
"####,
        expect![[
            r##"OK (:provenance (:source "419755a43b35b001370df4c38842a7091273074b2eff5db0ccab26e63fe287dc" :git-version "git version 2.51.2" :git-sha "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352" :fixture (("README.md" "46300e5fe8eb4b4662be23293d39e88631b0f7eb45f842f0ecb776effddd56c6") ("src/main.js" "3a9d558ceb5e8705b70b5124c78a9a8eb4a7df219039a7aa987da16a707acb9d") ("src/other.js" "8506d195735f8ee0d9bd98ae3f1f2f953285bcb8dd67e7a69582442cde55770f") ("test/main.test.js" "4fc06e96e400a94d2141e7ce5a076b372435e4eaed339ac6e9a52cd2d4d1b42f"))) :result (:outside ((:input nil :query "Deploy界" :sources ((:name "Git Grep" :base nil :raw "" :process nil :candidates nil)) :action nil :selected nil :history nil :location nil)) :recovery ((:input nil :query "Deploy界" :sources ((:name "Git Grep" :base "./" :raw "README.md\0001\0# Deploy界 runbook\nsrc/main.js\0002\0  return \"Deploy界\";\nsrc/other.js\0001\0export const fallback = \"deploy界 fallback\";\ntest/main.test.js\0001\0test(\"Deploy界\", () => true);\n" :process (:status exit :exit 0 :stable t) :candidates ((:display "README.md:1:# Deploy界 runbook" :faces ((:range (0 9) :text "README.md" :face helm-git-grep-file) (:range (10 11) :text "1" :face helm-git-grep-line) (:range (14 21) :text "Deploy界" :face (helm-match helm-git-grep-match))) :line 1 :content "# Deploy界 runbook" :file "README.md") (:display "src/main.js:2:  return \"Deploy界\";" :faces ((:range (0 11) :text "src/main.js" :face helm-git-grep-file) (:range (12 13) :text "2" :face helm-git-grep-line) (:range (24 31) :text "Deploy界" :face (helm-match helm-git-grep-match))) :line 2 :content "  return \"Deploy界\";" :file "src/main.js") (:display "src/other.js:1:export const fallback = \"deploy界 fallback\";" :faces ((:range (0 12) :text "src/other.js" :face helm-git-grep-file) (:range (13 14) :text "1" :face helm-git-grep-line) (:range (40 47) :text "deploy界" :face (helm-match helm-git-grep-match))) :line 1 :content "export const fallback = \"deploy界 fallback\";" :file "src/other.js") (:display "test/main.test.js:1:test(\"Deploy界\", () => true);" :faces ((:range (0 17) :text "test/main.test.js" :face helm-git-grep-file) (:range (18 19) :text "1" :face helm-git-grep-line) (:range (26 33) :text "Deploy界" :face (helm-match helm-git-grep-match))) :line 1 :content "test(\"Deploy界\", () => true);" :file "test/main.test.js")))) :action nil :selected nil :history nil :location nil))) :boundaries ((:kind process-file :cwd "../helm-git-grep-outside/" :program "git" :args ("--no-pager" "rev-parse" "--show-cdup") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8"))) (:kind process-file :cwd "./" :program "git" :args ("--no-pager" "rev-parse" "--show-cdup") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8"))) (:kind start-process :cwd "./" :program "git" :args ("--no-pager" "grep" "--null" "-n" "--no-color" "-i" "-e" "Deploy界") :environment (("GIT_CONFIG_NOSYSTEM" . "1") ("GIT_CONFIG_GLOBAL" . "/dev/null") ("GIT_OPTIONAL_LOCKS" . "0") ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8")))) :cleanup (:fixture-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :outside-root-exists nil :source-base-restored t :boundary-state-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        public_at_point_search_runs_real_git_and_navigates(),
        documented_option_toggles_rerun_the_public_search(),
        public_pathspec_listing_runs_real_git_and_displays_owned_files(),
        public_save_results_builds_grep_buffer_and_next_error_navigates(),
        outside_repository_empty_search_recovers_in_owned_repository(),
    ]
}

#[test]
fn public_helm_git_grep_workflows_match() {
    assert_oracle_batch_cases(oracle(), "helm-git-grep-rank394", "Helm Git Grep", &cases());
}
