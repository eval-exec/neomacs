//! Practical parity coverage for rank 413 `buttercup`.
//!
//! These cases drive Buttercup's public suite runner, lifecycle hooks,
//! expectations and pending specs, spy API, and recursive discovery runner.

use std::time::Duration;

use expect_test::expect;

use crate::{BUTTERCUP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'buttercup)

(get-buffer-create " *code-conversion-work*")

(defconst but413-test-upstream-main-sha
  "5f881beaa626990e81bf4654d10cf9972760d572d30bc1d5f1ee272bacfa5f4e")
(defconst but413-test-installed-main-sha
  "cab50b822486f5a911b547aa151f70e9c6df56612cf084a4e12a1b977f9c34ff")
(defconst but413-test-installed-compat-sha
  "e67ea7c58d5c732460949846c57ec55ca7ff76c35e68ba5856af7abf2911dd39")

(defvar but413-test-root nil)
(defvar but413-test-root-owned nil)
(defvar but413-test-ledger nil)
(defvar but413-test-discovered nil)
(defvar but413-test-spy-observation nil)

(defun but413-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun but413-test-source-state ()
  (let* ((main (file-truename (locate-library "buttercup.el")))
         (compat (file-truename (locate-library "buttercup-compat.el")))
         (manifest
          (list (cons "buttercup-compat.el" (but413-test-file-sha compat))
                (cons "buttercup.el" (but413-test-file-sha main)))))
    (unless (and (string-suffix-p "/buttercup.el" main)
                 (string-suffix-p "/buttercup-compat.el" compat)
                 (equal (file-name-directory main)
                        (file-name-directory compat))
                 (file-regular-p main)
                 (file-regular-p compat)
                 (not (file-symlink-p main))
                 (not (file-symlink-p compat))
                 (equal manifest
                        `(("buttercup-compat.el" .
                           ,but413-test-installed-compat-sha)
                          ("buttercup.el" .
                           ,but413-test-installed-main-sha))))
      (error "Buttercup installed source mismatch: %S" manifest))
    (list :upstream-sha256 but413-test-upstream-main-sha
          :installed-sha256 manifest
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'buttercup package-alist))))
          :feature (featurep 'buttercup))))

(defun but413-test-failure (value)
  (cond ((null value) nil)
        ((stringp value) value)
        ((and (consp value) (eq (car value) 'error))
         (list :error (cadr value)))
        (t (format "%S" value))))

(defun but413-test-node-state (node)
  (if (buttercup-spec-p node)
      (let ((status (buttercup-spec-status node)))
        (list :spec (buttercup-spec-description node)
              :full-name (buttercup-spec-full-name node)
              :status status
              :failure (and (memq status '(failed pending))
                            (but413-test-failure
                             (buttercup-spec-failure-description node)))))
    (list :suite (buttercup-suite-description node)
          :status (buttercup-suite-status node)
          :children (mapcar #'but413-test-node-state
                            (buttercup-suite-children node)))))

(defun but413-test-suites-state ()
  (mapcar #'but413-test-node-state buttercup-suites))

(defun but413-test-reporter (event argument)
  (push
   (pcase event
     ((or 'buttercup-started 'buttercup-done)
      (list event
            :defined (buttercup-suites-total-specs-defined argument)
            :pending (buttercup-suites-total-specs-pending argument)
            :failed (buttercup-suites-total-specs-failed argument)))
     ((or 'suite-started 'suite-done)
      (list event (buttercup-suite-description argument)
            (buttercup-suite-status argument)))
     ((or 'spec-started 'spec-done)
      (let ((status (buttercup-spec-status argument)))
        (list event (buttercup-spec-full-name argument)
              status
              (and (memq status '(failed pending))
                   (but413-test-failure
                    (buttercup-spec-failure-description argument))))))
     (_ (error "Unexpected Buttercup reporter event: %S" event)))
   but413-test-ledger))

(defun but413-test-write (relative contents)
  (let ((file (expand-file-name relative but413-test-root)))
    (unless (and but413-test-root-owned
                 (file-in-directory-p file but413-test-root))
      (error "Refusing Buttercup write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun but413-test-manifest (root)
  (sort
   (mapcar (lambda (file)
             (unless (and (file-regular-p file)
                          (not (file-symlink-p file)))
               (error "Unexpected Buttercup fixture entry: %s" file))
             (cons (file-relative-name file root)
                   (but413-test-file-sha file)))
           (directory-files-recursively root "."))
   (lambda (left right) (string< (car left) (car right)))))

(defun but413-test-target (name count)
  (format "original:%s:%d" name count))

(defun but413-test-forbid-external (operation &rest arguments)
  (error "Unexpected Buttercup external boundary: %S %S"
         operation arguments))

(defmacro but413-test-with-framework-debugger (&rest body)
  "Let Buttercup's dynamically installed debugger see BODY signals.
The parity adapter catches each case's final signal, so its outer handler would
otherwise prevent Buttercup's public runner from classifying failed and pending
specs through `buttercup--debugger'."
  (declare (indent 0))
  `(let ((debug-on-signal t)) ,@body))

(defun but413-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun but413-test-run (case-name body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (concat "buttercup-" case-name "/")
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (source-before (but413-test-source-state))
         (parked nil)
         (but413-test-root root)
         (but413-test-root-owned nil)
         (but413-test-ledger nil)
         (but413-test-discovered nil)
         (but413-test-spy-observation nil)
         (buttercup-suites nil)
         (buttercup--current-suite nil)
         (buttercup--before-each nil)
         (buttercup--after-each nil)
         (buttercup--cleanup-functions :inactive)
         (buttercup--spy-contexts (make-hash-table :test 'eq :weakness 'key))
         (buttercup-reporter #'but413-test-reporter)
         (buttercup-stack-frame-style 'omit)
         (buttercup-color nil)
         (buttercup-reporter-batch-quiet-statuses nil)
         (buttercup-reporter-batch--start-time nil)
         (buttercup-reporter-batch--failures nil)
         (buttercup-reporter-batch--suite-stack nil)
         (backtrace-on-error-noninteractive nil)
         (command-line-args-left nil)
         (lexical-binding t)
         result cleanup-errors source-after)
    (unwind-protect
        (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Buttercup sandbox root"))
          (when (file-exists-p root)
            (error "Buttercup sandbox root exists: %s" root))
          (when-let ((entry (but413-test-park-buffer
                             buttercup-warning-buffer-name)))
            (push entry parked))
          (make-directory root)
          (setq but413-test-root-owned t)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'call-process arguments)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'call-process-region arguments)))
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'make-process arguments)))
                    ((symbol-function 'process-file)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'process-file arguments)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'start-file-process arguments)))
                    ((symbol-function 'start-process)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'start-process arguments)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'url-retrieve arguments)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest arguments)
                       (apply #'but413-test-forbid-external
                              'url-retrieve-synchronously arguments))))
            (setq result (funcall body root)))
          (setq source-after (but413-test-source-state))
          (unless (equal source-before source-after)
            (error "Buttercup source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition)
                            (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (or (memq buffer buffers-before) (assq buffer parked))
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (dolist (entry parked)
          (attempt (list 'parked (cdr entry))
                   (lambda ()
                     (if (buffer-live-p (car entry))
                         (with-current-buffer (car entry)
                           (rename-buffer (cdr entry) t))
                       (error "Parked Buttercup buffer died: %s" (cdr entry))))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when but413-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer)
                                       (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer)
                                       (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame)
                                       (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Buttercup workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BUTTERCUP_MELPA_PIN, "buttercup.el")
        .expect("prepare exact Buttercup source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_runner_orders_nested_lifecycle_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_runner_orders_nested_lifecycle_hooks",
        r####"
(but413-test-run
 "lifecycle"
 (lambda (_root)
   (let (events)
     (describe "Release 界"
       (before-all (push 'before-all events))
       (after-all (push 'after-all events))
       (before-each (push 'before-each events))
       (after-each (push 'after-each events))
       (it "formats Unicode"
         (push 'first-body events)
         (expect (upcase "café 界") :to-equal "CAFÉ 界"))
       (describe "nested delivery"
         (it "inherits hooks"
           (push 'nested-body events)
           (expect '(notify publish) :to-contain 'publish))))
     (list :run (but413-test-with-framework-debugger (buttercup-run t))
           :events (nreverse events)
           :suites (but413-test-suites-state)
           :reporter (nreverse but413-test-ledger)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "5f881beaa626990e81bf4654d10cf9972760d572d30bc1d5f1ee272bacfa5f4e" :installed-sha256 (("buttercup-compat.el" . "e67ea7c58d5c732460949846c57ec55ca7ff76c35e68ba5856af7abf2911dd39") ("buttercup.el" . "cab50b822486f5a911b547aa151f70e9c6df56612cf084a4e12a1b977f9c34ff")) :version "20260512.2141" :feature t) :result (:run t :events (before-all before-each first-body after-each before-each nested-body after-each after-all) :suites ((:suite "Release 界" :status passed :children ((:spec "formats Unicode" :full-name "Release 界 formats Unicode" :status passed :failure nil) (:suite "nested delivery" :status passed :children ((:spec "inherits hooks" :full-name "Release 界 nested delivery inherits hooks" :status passed :failure nil)))))) :reporter ((buttercup-started :defined 2 :pending 0 :failed 0) (suite-started "Release 界" passed) (spec-started "Release 界 formats Unicode" passed nil) (spec-done "Release 界 formats Unicode" passed nil) (suite-started "nested delivery" passed) (spec-started "Release 界 nested delivery inherits hooks" passed nil) (spec-done "Release 界 nested delivery inherits hooks" passed nil) (suite-done "nested delivery" passed) (suite-done "Release 界" passed) (buttercup-done :defined 2 :pending 0 :failed 0))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_runner_reports_failure_pending_and_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_runner_reports_failure_pending_and_recovers",
        r####"
(but413-test-run
 "failure"
 (lambda (_root)
   (describe "Validation"
     (it "passes exact matcher"
       (expect "release-界" :to-match "界$"))
     (it "shows a distinguishing failure"
       (expect '(alpha beta) :to-equal '(alpha gamma)))
     (it "is pending")
     (xit "is disabled" (error "must not run")))
   (let ((first-run (but413-test-with-framework-debugger (buttercup-run t)))
         (first-state (but413-test-suites-state))
         (first-reporter (nreverse but413-test-ledger)))
     (setq buttercup-suites nil
           but413-test-ledger nil)
     (describe "Recovery"
       (it "runs after failure"
         (expect (+ 20 22) :to-be 42)))
     (list :first-run first-run
           :first-state first-state
           :first-reporter first-reporter
           :recovery-run
           (but413-test-with-framework-debugger (buttercup-run t))
           :recovery-state (but413-test-suites-state)
           :recovery-reporter (nreverse but413-test-ledger)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "5f881beaa626990e81bf4654d10cf9972760d572d30bc1d5f1ee272bacfa5f4e" :installed-sha256 (("buttercup-compat.el" . "e67ea7c58d5c732460949846c57ec55ca7ff76c35e68ba5856af7abf2911dd39") ("buttercup.el" . "cab50b822486f5a911b547aa151f70e9c6df56612cf084a4e12a1b977f9c34ff")) :version "20260512.2141" :feature t) :result (:first-run nil :first-state ((:suite "Validation" :status passed :children ((:spec "passes exact matcher" :full-name "Validation passes exact matcher" :status passed :failure nil) (:spec "shows a distinguishing failure" :full-name "Validation shows a distinguishing failure" :status failed :failure "Expected `'(alpha beta)' to be `equal' to `(alpha gamma)', but instead it was `(alpha beta)' which does not match because: (list-elt 1 (different-atoms beta gamma)).") (:spec "is pending" :full-name "Validation is pending" :status pending :failure "PENDING") (:spec "is disabled" :full-name "Validation is disabled" :status pending :failure "PENDING")))) :first-reporter ((buttercup-started :defined 4 :pending 2 :failed 0) (suite-started "Validation" passed) (spec-started "Validation passes exact matcher" passed nil) (spec-done "Validation passes exact matcher" passed nil) (spec-started "Validation shows a distinguishing failure" passed nil) (spec-done "Validation shows a distinguishing failure" failed "Expected `'(alpha beta)' to be `equal' to `(alpha gamma)', but instead it was `(alpha beta)' which does not match because: (list-elt 1 (different-atoms beta gamma)).") (spec-started "Validation is pending" pending "") (spec-done "Validation is pending" pending "PENDING") (spec-started "Validation is disabled" pending "") (spec-done "Validation is disabled" pending "PENDING") (suite-done "Validation" passed) (buttercup-done :defined 4 :pending 2 :failed 1)) :recovery-run t :recovery-state ((:suite "Recovery" :status passed :children ((:spec "runs after failure" :full-name "Recovery runs after failure" :status passed :failure nil)))) :recovery-reporter ((buttercup-started :defined 1 :pending 0 :failed 0) (suite-started "Recovery" passed) (spec-started "Recovery runs after failure" passed nil) (spec-done "Recovery runs after failure" passed nil) (suite-done "Recovery" passed) (buttercup-done :defined 1 :pending 0 :failed 0))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_spy_tracks_calls_and_restores_target() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_spy_tracks_calls_and_restores_target",
        r####"
(but413-test-run
 "spy"
 (lambda (_root)
   (describe "Delivery spy"
     (it "records arguments and return values"
       (spy-on 'but413-test-target :and-call-fake
               (lambda (name count) (format "fake:%s:%d" name count)))
       (let ((first (but413-test-target "界" 2))
             (second (but413-test-target "café" 3)))
         (expect 'but413-test-target :to-have-been-called-times 2)
         (expect 'but413-test-target :to-have-been-called-with "界" 2)
         (setq but413-test-spy-observation
               (list :returns (list first second)
                     :count (spy-calls-count 'but413-test-target)
                     :args (spy-calls-all-args 'but413-test-target))))))
   (list :run (but413-test-with-framework-debugger (buttercup-run t))
         :spy but413-test-spy-observation
         :restored (but413-test-target "界" 2)
         :suites (but413-test-suites-state)
         :reporter (nreverse but413-test-ledger))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "5f881beaa626990e81bf4654d10cf9972760d572d30bc1d5f1ee272bacfa5f4e" :installed-sha256 (("buttercup-compat.el" . "e67ea7c58d5c732460949846c57ec55ca7ff76c35e68ba5856af7abf2911dd39") ("buttercup.el" . "cab50b822486f5a911b547aa151f70e9c6df56612cf084a4e12a1b977f9c34ff")) :version "20260512.2141" :feature t) :result (:run t :spy (:returns ("fake:界:2" "fake:café:3") :count 2 :args (("界" 2) ("café" 3))) :restored "original:界:2" :suites ((:suite "Delivery spy" :status passed :children ((:spec "records arguments and return values" :full-name "Delivery spy records arguments and return values" :status passed :failure nil)))) :reporter ((buttercup-started :defined 1 :pending 0 :failed 0) (suite-started "Delivery spy" passed) (spec-started "Delivery spy records arguments and return values" passed nil) (spec-done "Delivery spy records arguments and return values" passed nil) (suite-done "Delivery spy" passed) (buttercup-done :defined 1 :pending 0 :failed 0))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_discovery_loads_visible_tests_and_filters_patterns() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_discovery_loads_visible_tests_and_filters_patterns",
        r####"
(but413-test-run
 "discover"
 (lambda (root)
   (but413-test-write
    "tests/test-release.el"
    ";;; -*- lexical-binding: t; -*-\n(require 'buttercup)\n(setq but413-test-discovered (append but413-test-discovered '(release)))\n(describe \"Discovered release\"\n  (it \"Unicode path 界\" (expect (+ 1 2) :to-be 3))\n  (it \"ordinary path\" (expect t :to-be t)))\n")
   (but413-test-write
    "tests/nested/delivery-tests.el"
    ";;; -*- lexical-binding: t; -*-\n(require 'buttercup)\n(setq but413-test-discovered (append but413-test-discovered '(delivery)))\n(describe \"Discovered delivery\"\n  (it \"Unicode delivery 界\" (expect \"café\" :to-match \"é$\")))\n")
   (but413-test-write
    "tests/.hidden/test-hidden.el"
    ";;; -*- lexical-binding: t; -*-\n(setq but413-test-discovered (append but413-test-discovered '(hidden)))\n(error \"hidden test must not load\")\n")
   (let ((fixture-before (but413-test-manifest root))
         (default-directory root)
         (command-line-args-left
          (list "--pattern" "Unicode" (expand-file-name "tests" root))))
     (let ((run (but413-test-with-framework-debugger
                  (buttercup-run-discover))))
       (list :run run
             :loaded but413-test-discovered
             :args-left command-line-args-left
             :fixture-unchanged
             (equal fixture-before (but413-test-manifest root))
             :suites (but413-test-suites-state)
             :reporter (nreverse but413-test-ledger))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "5f881beaa626990e81bf4654d10cf9972760d572d30bc1d5f1ee272bacfa5f4e" :installed-sha256 (("buttercup-compat.el" . "e67ea7c58d5c732460949846c57ec55ca7ff76c35e68ba5856af7abf2911dd39") ("buttercup.el" . "cab50b822486f5a911b547aa151f70e9c6df56612cf084a4e12a1b977f9c34ff")) :version "20260512.2141" :feature t) :result (:run t :loaded (delivery release) :args-left nil :fixture-unchanged t :suites ((:suite "Discovered delivery" :status passed :children ((:spec "Unicode delivery 界" :full-name "Discovered delivery Unicode delivery 界" :status passed :failure nil))) (:suite "Discovered release" :status passed :children ((:spec "Unicode path 界" :full-name "Discovered release Unicode path 界" :status passed :failure nil) (:spec "ordinary path" :full-name "Discovered release ordinary path" :status pending :failure "SKIPPED")))) :reporter ((buttercup-started :defined 3 :pending 1 :failed 0) (suite-started "Discovered delivery" passed) (spec-started "Discovered delivery Unicode delivery 界" passed nil) (spec-done "Discovered delivery Unicode delivery 界" passed nil) (suite-done "Discovered delivery" passed) (suite-started "Discovered release" passed) (spec-started "Discovered release Unicode path 界" passed nil) (spec-done "Discovered release Unicode path 界" passed nil) (spec-started "Discovered release ordinary path" pending nil) (spec-done "Discovered release ordinary path" pending "SKIPPED") (suite-done "Discovered release" passed) (buttercup-done :defined 3 :pending 1 :failed 0))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn buttercup_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_runner_orders_nested_lifecycle_hooks(),
        public_runner_reports_failure_pending_and_recovers(),
        public_spy_tracks_calls_and_restores_target(),
        public_discovery_loads_visible_tests_and_filters_patterns(),
    ];
    assert_oracle_batch_cases(oracle(), "buttercup-rank413", "buttercup_parity", &cases);
}
