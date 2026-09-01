//! Practical parity for esup's public Emacs startup profiler.
//!
//! These cases profile real init files through the child runner (network
//! and `kill-emacs` stubbed at the process boundary), launch `esup` with
//! recorded child argv, render sorted results, drop insignificant times,
//! show child errors, and visit a Unicode-named sexp.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ESUP_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'esup)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst es438-test-tree
  "e243d51040ea9d1a6a28aee8a43fbcc8a3721b57")
(defconst es438-test-manifest
  '(("esup-child.el" . "1cef6ac3f2d3f68579b3f53ea8d515842d2d3baae0611b39fce86713ce5d5bee")
    ("esup-pkg.el" . "d6cb4723214e694561b6019bbf58b68cf8a88a45e50bc9011999a9bd1121e759")
    ("esup.el" . "8e48d5a14b5a85bdc8e9a52824632dbc633f4ca956359945fc2d24f8d98d4ae9")))

(defvar es438-test-case-index 0)
(defvar es438-test-root nil)
(defvar es438-test-root-owned nil)
(defvar es438-test-ledger nil)

(defun es438-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun es438-test-source-state ()
  (let* ((located (locate-library "esup.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (es438-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/esup.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car es438-test-manifest)))
      (error "Unexpected installed esup payload: %S" (or manifest files)))
    (dolist (entry es438-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (or (equal expected "PENDING")
                         (equal (es438-test-sha file) expected)))
          (error "Unexpected installed esup source: %S"
                 (cons entry manifest)))))
    (list :tree es438-test-tree
          :manifest manifest
          :feature (featurep 'esup)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'esup package-alist)))))))

(defun es438-test-forbid-external (operation &rest arguments)
  (error "Unexpected esup external boundary: %S %S" operation arguments))

(defun es438-test-mask (value)
  (cond
   ((and (stringp value) es438-test-root)
    (replace-regexp-in-string (regexp-quote es438-test-root)
                              "[SANDBOX]/" value t t))
   ((stringp value) (copy-sequence value))
   (t value)))

(defun es438-test-write (root name code)
  (let ((file (expand-file-name name root)))
    (make-directory (file-name-directory file) t)
    (write-region code nil file nil 'silent)
    file))

(defun es438-test-summarize (results root)
  (mapcar
   (lambda (result)
     (list :file (es438-test-mask
                  (file-relative-name (slot-value result 'file) root))
           :expr (substring-no-properties
                  (slot-value result 'expression-string))
           :line (slot-value result 'line-number)
           :start (slot-value result 'start-point)
           :end (slot-value result 'end-point)
           :gc (slot-value result 'gc-number)))
   results))

(defun es438-test-eval-arg (args)
  (seq-find (lambda (arg)
              (and (stringp arg) (string-prefix-p "--eval=" arg)))
            args))

(defun es438-test-profile (root file &optional depth)
  (let ((load-path (cons root load-path))
        (features-before (copy-sequence features))
        (esup-child-max-depth (or depth 1))
        (esup-child-current-depth 0)
        (esup-child-last-call-intercept-results nil))
    (cl-letf (((symbol-function 'esup-child-init-streams)
               (lambda (&rest _) nil))
              ((symbol-function 'esup-child-send-log)
               (lambda (&rest _) nil))
              ((symbol-function 'esup-child-send-results)
               (lambda (&rest _) nil))
              ((symbol-function 'esup-child-send-result-separator)
               (lambda (&rest _) nil))
              ((symbol-function 'esup-child-send-eof)
               (lambda (&rest _) nil))
              ((symbol-function 'kill-emacs)
               (lambda (&rest _) nil))
              ((symbol-function 'find-file-noselect)
               (lambda (filename &rest _)
                 (let ((buf (generate-new-buffer
                             (file-name-nondirectory filename))))
                   (with-current-buffer buf
                     (setq buffer-file-name filename)
                     (insert-file-contents filename)
                     (fundamental-mode)
                     buf)))))
      (unwind-protect
          (esup-child-run (expand-file-name file root) -1 (or depth 1))
        (setq features features-before
              esup-child-current-depth 0
              esup-child-last-call-intercept-results nil)
        (advice-remove 'require 'esup-child-require-advice)
        (advice-remove 'load 'esup-child-load-advice)))))

(defun es438-test-run (body)
  (let* ((index (cl-incf es438-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "esup-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (es438-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (features-before (copy-sequence features))
         (init-before esup-user-init-file)
         (depth-before esup-depth)
         (batch-before esup-run-as-batch-p)
         (insignificant-before esup-insignificant-time)
         (port-before esup-server-port)
         (errors-before (copy-sequence esup-errors))
         (child-before esup-child-process)
         (server-before esup-server-process)
         (child-depth-before esup-child-max-depth)
         (child-current-before esup-child-current-depth)
         (intercept-before esup-child-last-call-intercept-results)
         (last-start-before esup-last-result-start-point)
         (es438-test-root root)
         (es438-test-root-owned nil)
         (es438-test-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute esup sandbox root"))
              (when (file-exists-p root)
                (error "esup sandbox root exists: %S" root))
              (make-directory root)
              (setq es438-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'make-network-process args)))
                        ((symbol-function 'open-network-stream)
                         (lambda (&rest args)
                           (apply #'es438-test-forbid-external
                                  'open-network-stream args))))
                (setq result (funcall body root)))
              (setq source-after (es438-test-source-state))
              (unless (equal source-before source-after)
                (error "esup source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (advice-remove 'require 'esup-child-require-advice)
        (advice-remove 'load 'esup-child-load-advice)
        (setq esup-user-init-file init-before
              esup-depth depth-before
              esup-run-as-batch-p batch-before
              esup-insignificant-time insignificant-before
              esup-server-port port-before
              esup-errors errors-before
              esup-child-process child-before
              esup-server-process server-before
              esup-child-max-depth child-depth-before
              esup-child-current-depth child-current-before
              esup-child-last-call-intercept-results intercept-before
              esup-last-result-start-point last-start-before
              features features-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when es438-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "esup body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "esup cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ESUP_MELPA_PIN, "esup.el")
        .expect("prepare pinned esup source below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn profiles_init_require_chain_and_counts_gc() -> ParityBatchCase {
    ParityBatchCase::value(
        "profiles_init_require_chain_and_counts_gc",
        r####"
(es438-test-run
 (lambda (root)
   (es438-test-write
    root "init.el"
    "(progn 'hello-café)\n(require 'helper)\n(progn (garbage-collect) (garbage-collect))\n")
   (es438-test-write
    root "helper.el"
    "(progn 'helper-界)\n(provide 'helper)\n")
   (es438-test-write root "empty.el" "")
   (es438-test-write
    root "deep-a.el" "(require 'deep-c)\n")
   (es438-test-write
    root "deep-c.el" "(require 'deep-d)\n")
   (es438-test-write
    root "deep-d.el" "(progn 'deep)\n")
   (list :depth1 (es438-test-summarize (es438-test-profile root "init.el" 1) root)
         :depth0 (es438-test-summarize (es438-test-profile root "init.el" 0) root)
         :chain1 (es438-test-summarize (es438-test-profile root "deep-a.el" 1) root)
         :empty (es438-test-summarize (es438-test-profile root "empty.el" 1) root)
         :missing (es438-test-summarize
                   (es438-test-profile root "no-such.el" 1)
                   root))))
"####,
        expect![[
            r#"OK (:source (:tree "e243d51040ea9d1a6a28aee8a43fbcc8a3721b57" :manifest (("esup-child.el" . "1cef6ac3f2d3f68579b3f53ea8d515842d2d3baae0611b39fce86713ce5d5bee") ("esup-pkg.el" . "d6cb4723214e694561b6019bbf58b68cf8a88a45e50bc9011999a9bd1121e759") ("esup.el" . "8e48d5a14b5a85bdc8e9a52824632dbc633f4ca956359945fc2d24f8d98d4ae9")) :feature t :version "20220202.2335") :result (:depth1 ((:file "init.el" :expr "(progn 'hello-café)" :line 1 :start 1 :end 20 :gc 0) (:file "helper.el" :expr "(progn 'helper-界)" :line 1 :start 1 :end 18 :gc 0) (:file "helper.el" :expr "(provide 'helper)" :line 2 :start 19 :end 36 :gc 0) (:file "init.el" :expr "(progn (garbage-collect) (garbage-collect))" :line 3 :start 39 :end 82 :gc 2)) :depth0 ((:file "init.el" :expr "(progn 'hello-café)" :line 1 :start 1 :end 20 :gc 0) (:file "init.el" :expr "(require 'helper)" :line 2 :start 21 :end 38 :gc 0) (:file "init.el" :expr "(progn (garbage-collect) (garbage-collect))" :line 3 :start 39 :end 82 :gc 2)) :chain1 ((:file "deep-a.el" :expr "(require 'deep-c)" :line 1 :start 1 :end 18 :gc 0)) :empty nil :missing nil) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn esup_starts_child_with_init_depth_and_batch() -> ParityBatchCase {
    ParityBatchCase::value(
        "esup_starts_child_with_init_depth_and_batch",
        r####"
(es438-test-run
 (lambda (root)
   (let ((init (es438-test-write root "startup.el" "(progn 'ok)\n"))
         (prompted (es438-test-write root "café-init.el" "(progn 'prompt)\n"))
         launched)
     (cl-letf (((symbol-function 'make-network-process)
                (lambda (&rest args)
                  (push (list :network (plist-get args :name)
                              :server (plist-get args :server)
                              :host (plist-get args :host)
                              :filter (plist-get args :filter))
                        es438-test-ledger)
                  'es438-fake-server))
               ((symbol-function 'process-contact)
                (lambda (proc &optional key)
                  (if (eq key :service) 4242 t)))
               ((symbol-function 'start-process)
                (lambda (&rest args)
                  (push (list :start (mapcar #'es438-test-mask args))
                        es438-test-ledger)
                  'es438-fake-child))
               ((symbol-function 'set-process-sentinel)
                (lambda (&rest _) nil))
               ((symbol-function 'delete-process)
                (lambda (&rest _) nil))
               ((symbol-function 'read-file-name)
                (lambda (&rest _) prompted)))
       (setq esup-depth 2
             esup-run-as-batch-p nil)
       (esup init)
       (let* ((first (car es438-test-ledger))
              (args (and (eq (car first) :start) (cadr first)))
              (eval-arg (es438-test-eval-arg args)))
         (setq launched
               (list :emacs-is-invocation
                     (equal (nth 2 args) esup-emacs-path)
                     :q (and (member "-q" args) t)
                     :load-child (equal (nth 1 (member "-l" args))
                                        "esup-child")
                     :load-path-flag (and (member "-L" args) t)
                     :eval (es438-test-mask eval-arg)
                     :port esup-server-port
                     :batch (and (member "--batch" args) t))))
       (setq esup-run-as-batch-p t)
       (esup init)
       (let* ((second (car es438-test-ledger))
              (args (and (eq (car second) :start) (cadr second))))
         (setq launched
               (append launched
                       (list :batch-after (and (member "--batch" args) t)))))
       (esup '(4))
       (let* ((third (car es438-test-ledger))
              (args (and (eq (car third) :start) (cadr third)))
              (eval-arg (es438-test-eval-arg args)))
         (list :launched launched
               :prompted-eval (es438-test-mask eval-arg)
               :ledger-kinds (mapcar #'car (reverse es438-test-ledger))))))))
"####,
        expect![[
            r#"OK (:source (:tree "e243d51040ea9d1a6a28aee8a43fbcc8a3721b57" :manifest (("esup-child.el" . "1cef6ac3f2d3f68579b3f53ea8d515842d2d3baae0611b39fce86713ce5d5bee") ("esup-pkg.el" . "d6cb4723214e694561b6019bbf58b68cf8a88a45e50bc9011999a9bd1121e759") ("esup.el" . "8e48d5a14b5a85bdc8e9a52824632dbc633f4ca956359945fc2d24f8d98d4ae9")) :feature t :version "20220202.2335") :result (:launched (:emacs-is-invocation t :q t :load-child t :load-path-flag t :eval "--eval=(esup-child-run \"[SANDBOX]/startup.el\" \"4242\" 2)" :port 4242 :batch nil :batch-after t) :prompted-eval "--eval=(esup-child-run \"[SANDBOX]/café-init.el\" \"4242\" 2)" :ledger-kinds (:network :start :network :start :network :start)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn renders_sorted_results_and_navigates() -> ParityBatchCase {
    ParityBatchCase::value(
        "renders_sorted_results_and_navigates",
        r####"
(es438-test-run
 (lambda (root)
   (let* ((slow (es438-test-write root "slow.el" "(require 'café)\n"))
          (fast (es438-test-write root "fast.el" "(setq x 1)\n"))
          (results
           (list
            (esup-result :file slow
                         :expression-string "(require 'café)"
                         :start-point 1 :end-point 16 :line-number 1
                         :exec-time 0.150 :gc-number 1 :gc-time 0.010)
            (esup-result :file fast
                         :expression-string "(setq x 1)"
                         :start-point 1 :end-point 11 :line-number 1
                         :exec-time 0.050 :gc-number 0 :gc-time 0.0))))
     (esup-reset)
     (setq esup-errors nil)
     (cl-letf (((symbol-function 'esup-read-results) (lambda () results)))
       (esup-display-results))
     (with-current-buffer (esup-buffer)
       (let ((text (buffer-substring-no-properties (point-min) (point-max)))
             (mode major-mode)
             (at-min (point)))
         (esup-next-result)
         (let ((after-n (point))
               (file-face (get-text-property (point) 'font-lock-face))
               (full (es438-test-mask (get-text-property (point) 'full-file))))
           (esup-next-result)
           (let ((after-n2 (point)))
             (esup-previous-result)
             (list :text (es438-test-mask text)
                   :mode mode
                   :at-min at-min
                   :after-n after-n
                   :after-n2 after-n2
                   :back (point)
                   :file-face file-face
                   :full full
                   :read-only buffer-read-only))))))))
"####,
        expect![[
            r#"OK (:source (:tree "e243d51040ea9d1a6a28aee8a43fbcc8a3721b57" :manifest (("esup-child.el" . "1cef6ac3f2d3f68579b3f53ea8d515842d2d3baae0611b39fce86713ce5d5bee") ("esup-pkg.el" . "d6cb4723214e694561b6019bbf58b68cf8a88a45e50bc9011999a9bd1121e759") ("esup.el" . "8e48d5a14b5a85bdc8e9a52824632dbc633f4ca956359945fc2d24f8d98d4ae9")) :feature t :version "20220202.2335") :result (:text "Total User Startup Time: 0.200sec     Total Number of GC Pauses: 1     Total GC Time: 0.010sec\n\nslow.el:1  0.150sec   74%\n(require 'café)\n\nfast.el:1  0.050sec   25%\n(setq x 1)\n\n" :mode esup-mode :at-min 1 :after-n 97 :after-n2 140 :back 97 :file-face esup-file :full "[SANDBOX]/slow.el" :read-only t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn drops_insignificant_shows_errors_and_visits_item() -> ParityBatchCase {
    ParityBatchCase::value(
        "drops_insignificant_shows_errors_and_visits_item",
        r####"
(es438-test-run
 (lambda (root)
   (let* ((kept (es438-test-write
                 root "café/keep.el" "(progn 'keep)\n"))
          (tiny (es438-test-write root "tiny.el" "(progn 'tiny)\n"))
          (results
           (list
            (esup-result :file tiny
                         :expression-string "(progn 'tiny)"
                         :start-point 1 :end-point 14 :line-number 1
                         :exec-time 0.001 :gc-number 0 :gc-time 0.0)
            (esup-result :file kept
                         :expression-string "(progn 'keep)"
                         :start-point 1 :end-point 14 :line-number 1
                         :exec-time 0.100 :gc-number 0 :gc-time 0.0))))
     (esup-reset)
     (setq esup-insignificant-time 0.009
           esup-errors '("ERROR child exploded"))
     (cl-letf (((symbol-function 'esup-read-results) (lambda () results)))
       (esup-display-results))
     (set-buffer (esup-buffer))
     (goto-char (point-min))
     (re-search-forward "keep.el")
     (goto-char (match-beginning 0))
     (esup-visit-item)
     (list :text (es438-test-mask
                  (with-current-buffer esup-display-buffer
                    (buffer-substring-no-properties (point-min) (point-max))))
           :visited (list :file (es438-test-mask
                                 (file-relative-name buffer-file-name root))
                          :point (point)
                          :looking (buffer-substring-no-properties
                                    (point) (line-end-position)))
           :error-face
           (with-current-buffer esup-display-buffer
             (goto-char (point-min))
             (re-search-forward "ERROR")
             (face-at-point))))))
"####,
        expect![[
            r#"OK (:source (:tree "e243d51040ea9d1a6a28aee8a43fbcc8a3721b57" :manifest (("esup-child.el" . "1cef6ac3f2d3f68579b3f53ea8d515842d2d3baae0611b39fce86713ce5d5bee") ("esup-pkg.el" . "d6cb4723214e694561b6019bbf58b68cf8a88a45e50bc9011999a9bd1121e759") ("esup.el" . "8e48d5a14b5a85bdc8e9a52824632dbc633f4ca956359945fc2d24f8d98d4ae9")) :feature t :version "20220202.2335") :result (:text "ERROR: the child emacs had the following errors:\n  ERROR child exploded\n\nResults will be incomplete due to errors.\n\n\nTotal User Startup Time: 0.101sec     Total Number of GC Pauses: 0     Total GC Time: 0.000sec\n\nkeep.el:1  0.100sec   99%\n(progn 'keep)\n\ntiny.el:1  0.001sec   0%\n(progn 'tiny)\n\n" :visited (:file "café/keep.el" :point 1 :looking "(progn 'keep)") :error-face nil) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn esup_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        profiles_init_require_chain_and_counts_gc(),
        esup_starts_child_with_init_depth_and_batch(),
        renders_sorted_results_and_navigates(),
        drops_insignificant_shows_errors_and_visits_item(),
    ];
    assert_oracle_batch_cases(oracle(), "esup-rank438", "esup_parity", &cases);
}
