//! Practical parity coverage for rank 415 `el-mock`.
//!
//! These cases drive the public stub, mock, not-called, `with-mock`, and
//! `mocklet` interfaces through successful verification, exact failures, and
//! teardown after both normal and nonlocal exits.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EL_MOCK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'el-mock)

(get-buffer-create " *code-conversion-work*")

(defconst elm415-test-upstream-main-sha
  "6988ece4f269c1d3688b7374175c936dd4fb2f95af1416906aad53164fa1bf61")
(defconst elm415-test-installed-main-sha
  "1b6d3ed24b3abd32d5cb0d584e776c7931b061552b12fc98fbe0e386baa86510")
(defconst elm415-test-installed-pkg-sha
  "80bfd9f5970145dedbae4e661a8ccc08d9ccfa1d788cc1dc8059cb3fb2e10ab5")

(defvar elm415-expected nil)
(defconst elm415-test-owned-symbols
  '(elm415-target elm415-undefined elm415-forbidden elm415-expected))

(defun elm415-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun elm415-test-source-state ()
  (let* ((main (file-truename (locate-library "el-mock.el")))
         (directory (file-name-directory main))
         (pkg (expand-file-name "el-mock-pkg.el" directory))
         (manifest
          (list (cons "el-mock-pkg.el" (elm415-test-file-sha pkg))
                (cons "el-mock.el" (elm415-test-file-sha main)))))
    (unless (and (string-suffix-p "/el-mock.el" main)
                 (file-regular-p main)
                 (file-regular-p pkg)
                 (not (file-symlink-p main))
                 (not (file-symlink-p pkg))
                 (equal manifest
                        `(("el-mock-pkg.el" . ,elm415-test-installed-pkg-sha)
                          ("el-mock.el" . ,elm415-test-installed-main-sha))))
      (error "El Mock installed source mismatch: %S" manifest))
    (list :upstream-sha256 elm415-test-upstream-main-sha
          :installed-sha256 manifest
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'el-mock package-alist))))
          :feature (featurep 'el-mock))))

(defun elm415-test-symbol-state (symbol)
  (list :symbol symbol
        :fbound (fboundp symbol)
        :function (and (fboundp symbol) (symbol-function symbol))
        :bound (boundp symbol)
        :value (and (boundp symbol) (symbol-value symbol))
        :plist (copy-tree (symbol-plist symbol))))

(defun elm415-test-mock-properties (symbol)
  (list :has-original (and (get symbol 'mock-original-func) t)
        :call-count (get symbol 'mock-call-count)))

(defun elm415-test-restore-symbol (state)
  (let ((symbol (plist-get state :symbol)))
    (if (plist-get state :fbound)
        (fset symbol (plist-get state :function))
      (fmakunbound symbol))
    (if (plist-get state :bound)
        (set symbol (plist-get state :value))
      (makunbound symbol))
    (setplist symbol (copy-tree (plist-get state :plist)))))

(defun elm415-test-symbol-restored-p (state)
  (let ((symbol (plist-get state :symbol)))
    (and (eq (fboundp symbol) (plist-get state :fbound))
         (or (not (plist-get state :fbound))
             (eq (symbol-function symbol) (plist-get state :function)))
         (eq (boundp symbol) (plist-get state :bound))
         (or (not (plist-get state :bound))
             (eq (symbol-value symbol) (plist-get state :value)))
         (equal (symbol-plist symbol) (plist-get state :plist)))))

(defun elm415-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (copy-tree (cdr condition))
           :message (error-message-string condition)))))

(defun elm415-test-forbid-external (operation &rest arguments)
  (error "Unexpected El Mock external boundary: %S %S" operation arguments))

(defun elm415-test-run (body)
  (let* ((buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (source-before (elm415-test-source-state))
         (symbols-before (mapcar #'elm415-test-symbol-state
                                 elm415-test-owned-symbols))
         (-stubbed-functions nil)
         (-mocked-functions nil)
         (mock-verify-list nil)
         (in-mocking nil)
         result source-after cleanup-errors)
    (unwind-protect
        (progn
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'call-process arguments)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'call-process-region arguments)))
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'make-process arguments)))
                    ((symbol-function 'process-file)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'process-file arguments)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'start-file-process arguments)))
                    ((symbol-function 'start-process)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'start-process arguments)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'url-retrieve arguments)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest arguments)
                       (apply #'elm415-test-forbid-external
                              'url-retrieve-synchronously arguments))))
            (setq result (funcall body)))
          (setq source-after (elm415-test-source-state))
          (unless (equal source-before source-after)
            (error "El Mock source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (dolist (state symbols-before)
          (attempt (list 'symbol (plist-get state :symbol))
                   (lambda () (elm415-test-restore-symbol state))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :symbols-restored
                 (cl-every #'elm415-test-symbol-restored-p symbols-before)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "El Mock cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EL_MOCK_MELPA_PIN, "el-mock.el")
        .expect("prepare exact el-mock source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_with_mock_stubs_and_restores_defined_and_new_functions() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_with_mock_stubs_and_restores_defined_and_new_functions",
        r####"
(elm415-test-run
 (lambda ()
   (fset 'elm415-target
         (lambda (name) (format "original:%s" name)))
   (fmakunbound 'elm415-undefined)
   (let ((original (symbol-function 'elm415-target))
         inside first-properties second-value)
     (setq inside
           (with-mock
             (stub elm415-target => "stubbed 界")
             (stub elm415-undefined => '(created café))
             (list (elm415-target "ignored")
                   (elm415-undefined 1 2 3))))
     (setq first-properties (elm415-test-mock-properties 'elm415-target))
     (fmakunbound 'elm415-target)
     (setq second-value
           (with-mock
             (stub elm415-target => "second stub")
             (elm415-target 'inside-second)))
     (list :inside inside
           :first-properties first-properties
           :second-value second-value
           :stale-original-resurrected
           (eq (symbol-function 'elm415-target) original)
           :resurrected-value (elm415-target "after-second")
           :final-properties (elm415-test-mock-properties 'elm415-target)
           :new-function-removed (not (fboundp 'elm415-undefined))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "6988ece4f269c1d3688b7374175c936dd4fb2f95af1416906aad53164fa1bf61" :installed-sha256 (("el-mock-pkg.el" . "80bfd9f5970145dedbae4e661a8ccc08d9ccfa1d788cc1dc8059cb3fb2e10ab5") ("el-mock.el" . "1b6d3ed24b3abd32d5cb0d584e776c7931b061552b12fc98fbe0e386baa86510")) :version "20220625.1949" :feature t) :result (:inside ("stubbed 界" (created café)) :first-properties (:has-original t :call-count nil) :second-value "second stub" :stale-original-resurrected t :resurrected-value "original:after-second" :final-properties (:has-original t :call-count nil) :new-function-removed t) :cleanup (:source-unchanged t :symbols-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_mock_verifies_arguments_wildcards_and_times() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mock_verifies_arguments_wildcards_and_times",
        r####"
(elm415-test-run
 (lambda ()
   (fset 'elm415-target (lambda (&rest args) (cons 'original args)))
   (let ((elm415-expected 7)
         success wrong-argument wrong-times missing)
     (setq success
           (mapcar
            #'copy-tree
            (with-mock
              (mock (elm415-target elm415-expected *)
                    => '(verified 界) :times 2)
              (list (elm415-target 7 'alpha)
                    (elm415-target 7 '(beta café))))))
     (setq wrong-argument
           (elm415-test-condition
            (lambda ()
              (with-mock
                (mock (elm415-target elm415-expected *))
                (elm415-target 8 'alpha)))))
     (setq wrong-times
           (elm415-test-condition
            (lambda ()
              (with-mock
                (mock (elm415-target 7 *) :times 2)
                (elm415-target 7 'once)))))
     (setq missing
           (elm415-test-condition
            (lambda ()
              (with-mock
                (mock (elm415-target 7))
                'body-finished))))
     (list :success success
           :wrong-argument wrong-argument
           :wrong-times wrong-times
           :missing missing
           :properties (elm415-test-mock-properties 'elm415-target)
           :restored (equal (elm415-target 'after) '(original after))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "6988ece4f269c1d3688b7374175c936dd4fb2f95af1416906aad53164fa1bf61" :installed-sha256 (("el-mock-pkg.el" . "80bfd9f5970145dedbae4e661a8ccc08d9ccfa1d788cc1dc8059cb3fb2e10ab5") ("el-mock.el" . "1b6d3ed24b3abd32d5cb0d584e776c7931b061552b12fc98fbe0e386baa86510")) :version "20220625.1949" :feature t) :result (:success ((verified 界) (verified 界)) :wrong-argument (:error mock-error :data ((elm415-target elm415-expected *) (elm415-target 8 alpha)) :message "Mock error: (elm415-target elm415-expected *), (elm415-target 8 alpha)") :wrong-times (:error mock-error :data ((elm415-target 7 *) :expected-times 2 :actual-times 1) :message "Mock error: (elm415-target 7 *), :expected-times, 2, :actual-times, 1") :missing (:error mock-error :data (not-called elm415-target) :message "Mock error: not-called, elm415-target") :properties (:has-original t :call-count 0) :restored t) :cleanup (:source-unchanged t :symbols-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_mocklet_mixes_mock_stub_and_not_called_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mocklet_mixes_mock_stub_and_not_called_contracts",
        r####"
(elm415-test-run
 (lambda ()
   (mapc #'fmakunbound '(elm415-target elm415-undefined elm415-forbidden))
   (let ((success
          (mapcar
           #'copy-tree
           (mocklet (((elm415-target "café" *) => '(mocked 界) :times 2)
                     (elm415-undefined => "stub value")
                     (elm415-forbidden not-called))
             (list (elm415-target "café" 1)
                   (elm415-undefined :any)
                   (elm415-target "café" '(2 3))))))
         (forbidden
          (elm415-test-condition
           (lambda ()
             (mocklet ((elm415-forbidden not-called))
               (elm415-forbidden 'boom))))))
     (list :success success
           :forbidden forbidden
           :with-stub-alias (eq (symbol-function 'with-stub) 'with-mock)
           :stublet-alias (eq (symbol-function 'stublet) 'mocklet)
           :functions-removed
           (mapcar (lambda (symbol) (list symbol (not (fboundp symbol))))
                   '(elm415-target elm415-undefined elm415-forbidden))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "6988ece4f269c1d3688b7374175c936dd4fb2f95af1416906aad53164fa1bf61" :installed-sha256 (("el-mock-pkg.el" . "80bfd9f5970145dedbae4e661a8ccc08d9ccfa1d788cc1dc8059cb3fb2e10ab5") ("el-mock.el" . "1b6d3ed24b3abd32d5cb0d584e776c7931b061552b12fc98fbe0e386baa86510")) :version "20220625.1949" :feature t) :result (:success ((mocked 界) "stub value" (mocked 界)) :forbidden (:error mock-error :data (called) :message "Mock error: called") :with-stub-alias t :stublet-alias t :functions-removed ((elm415-target t) (elm415-undefined t) (elm415-forbidden t))) :cleanup (:source-unchanged t :symbols-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_with_mock_tears_down_before_propagating_body_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_with_mock_tears_down_before_propagating_body_failure",
        r####"
(elm415-test-run
 (lambda ()
   (fset 'elm415-target
         (lambda (value) (format "original:%s" value)))
   (let* ((original (symbol-function 'elm415-target))
          (failure
           (elm415-test-condition
            (lambda ()
              (with-mock
                (stub elm415-target => "temporary")
                (unless (equal (elm415-target 'inside) "temporary")
                  (error "Stub was not active"))
                (error "body failed 界"))))))
     (list :failure failure
           :restored (eq (symbol-function 'elm415-target) original)
           :value (elm415-target 'after)
           :verification-state
           (list :stubbed -stubbed-functions
                 :mocked -mocked-functions
                 :verify mock-verify-list
                 :in-mocking in-mocking)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "6988ece4f269c1d3688b7374175c936dd4fb2f95af1416906aad53164fa1bf61" :installed-sha256 (("el-mock-pkg.el" . "80bfd9f5970145dedbae4e661a8ccc08d9ccfa1d788cc1dc8059cb3fb2e10ab5") ("el-mock.el" . "1b6d3ed24b3abd32d5cb0d584e776c7931b061552b12fc98fbe0e386baa86510")) :version "20220625.1949" :feature t) :result (:failure (:error error :data ("body failed 界") :message "body failed 界") :restored t :value "original:after" :verification-state (:stubbed nil :mocked nil :verify nil :in-mocking nil)) :cleanup (:source-unchanged t :symbols-restored t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn el_mock_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_with_mock_stubs_and_restores_defined_and_new_functions(),
        public_mock_verifies_arguments_wildcards_and_times(),
        public_mocklet_mixes_mock_stub_and_not_called_contracts(),
        public_with_mock_tears_down_before_propagating_body_failure(),
    ];
    assert_oracle_batch_cases(oracle(), "el-mock-rank415", "el_mock_parity", &cases);
}
