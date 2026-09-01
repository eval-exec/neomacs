//! Practical parity for go-eldoc's public go-mode eldoc setup.
//!
//! These cases enable eldoc through `go-eldoc-setup`, format gocode
//! signatures and builtin make/len fallbacks, document variables and
//! assignment returns, and recover after a missing gocode.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GO_ELDOC_MELPA_PIN, GO_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'go-mode)
(require 'go-eldoc)
(set-window-configuration (current-window-configuration))

(defconst ge434-test-tree
  "99067a37568b83720e5c91c96f8ecbebed5ecd20")
(defconst ge434-test-manifest
  '(("go-eldoc-pkg.el" . "6b1dd121ad4c597109545f58fee37d3f437a655310c8dd2cbd652621d41821bb")
    ("go-eldoc.el" . "5e1fce7fd467d0d2a85ff6072c0f600de588122fbec9674c97e45b72412f4bc9")))

(defvar ge434-test-case-index 0)
(defvar ge434-test-root nil)
(defvar ge434-test-root-owned nil)
(defvar ge434-test-gocode-plan nil)
(defvar ge434-test-gocode-ledger nil)

(defun ge434-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ge434-test-source-state ()
  (let* ((located (locate-library "go-eldoc.el"))
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
                         (cons file (ge434-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/go-eldoc.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car ge434-test-manifest)))
      (error "Unexpected installed go-eldoc payload: %S" (or manifest files)))
    (dolist (entry ge434-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ge434-test-sha file) expected))
          (error "Unexpected installed go-eldoc source: %S"
                 (cons entry manifest)))))
    (list :tree ge434-test-tree
          :manifest manifest
          :feature (featurep 'go-eldoc)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'go-eldoc package-alist)))))))

(defun ge434-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (copy-sequence item)
                             (copy-tree item)))
                         (cdr condition))
           :message (copy-sequence (error-message-string condition))))))

(defun ge434-test-forbid-external (operation &rest arguments)
  (error "Unexpected go-eldoc external boundary: %S %S" operation arguments))

(defun ge434-test-mask (value)
  (cond
   ((and (stringp value) ge434-test-root)
    (replace-regexp-in-string (regexp-quote ge434-test-root)
                              "[SANDBOX]/" value t t))
   ((stringp value) (copy-sequence value))
   (t value)))

(defun ge434-test-call-process-region
    (start end program &optional _delete dest _display &rest args)
  (push (list :program (copy-sequence program)
              :args (mapcar #'ge434-test-mask args)
              :file (ge434-test-mask (or (buffer-file-name) "")))
        ge434-test-gocode-ledger)
  (unless (equal program go-eldoc-gocode)
    (apply #'ge434-test-forbid-external 'call-process-region
           program args))
  (cond
   ((eq ge434-test-gocode-plan :missing)
    (error "Searching for program: no such file or directory, %s" program))
   ((null ge434-test-gocode-plan)
    (error "Unexpected go-eldoc gocode: %S %S" program args))
   (t
    (let ((output (pop ge434-test-gocode-plan)))
      (when (bufferp dest)
        (with-current-buffer dest
          (insert (or output ""))))
      0))))

(defun ge434-test-faces (text)
  (let ((pos 0)
        spans)
    (while (< pos (length text))
      (let* ((face (get-text-property pos 'face text))
             (end (or (next-single-property-change pos 'face text)
                      (length text))))
        (when face
          (push (list :from pos :to end :face face
                      :part (substring-no-properties text pos end))
                spans))
        (setq pos end)))
    (nreverse spans)))

(defun ge434-test-doc ()
  (let ((text (funcall eldoc-documentation-function)))
    (if (not (stringp text))
        text
      (list :text (substring-no-properties text)
            :faces (ge434-test-faces text)))))

(defun ge434-test-visit (root name code)
  (let ((file (expand-file-name name root)))
    (write-region code nil file nil 'silent)
    (find-file file)
    (go-mode)
    (font-lock-fontify-buffer)
    (go-eldoc-setup)
    file))

(defun ge434-test-at (pattern)
  (goto-char (point-min))
  (re-search-forward pattern)
  (goto-char (match-beginning 0))
  (ge434-test-doc))

(defun ge434-test-run (body)
  (let* ((index (cl-incf ge434-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "go-eldoc-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (ge434-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (gocode-before go-eldoc-gocode)
         (gocode-args-before go-eldoc-gocode-args)
         (ge434-test-root root)
         (ge434-test-root-owned nil)
         (ge434-test-gocode-plan nil)
         (ge434-test-gocode-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute go-eldoc sandbox root"))
              (when (file-exists-p root)
                (error "go-eldoc sandbox root exists: %S" root))
              (make-directory root)
              (setq ge434-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         #'ge434-test-call-process-region)
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'process-lines)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'process-lines args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'ge434-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (setq source-after (ge434-test-source-state))
              (unless (equal source-before source-after)
                (error "go-eldoc source changed")))
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
        (setq go-eldoc-gocode gocode-before
              go-eldoc-gocode-args gocode-args-before
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
        (when ge434-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "go-eldoc body failed: %S" body-error))
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
          (error "go-eldoc cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GO_ELDOC_MELPA_PIN, "go-eldoc.el")
        .expect("prepare pinned go-eldoc source below ./tmp")
        .with_melpa_dependency(GO_MODE_MELPA_PIN)
        .expect("prepare pinned go-mode dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn setup_enables_eldoc_and_documents_gocode_functions() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_enables_eldoc_and_documents_gocode_functions",
        r####"
(ge434-test-run
 (lambda (root)
   (setq go-eldoc-gocode "gocode"
         go-eldoc-gocode-args '("-in=text"))
   (ge434-test-visit
    root "call.go"
    "package main

func foo(arg1 int, arg2 string) (int, error) {
        return 0, nil
}

func main() {
        foo(1, \"x\")
}
")
   (let* ((setup (list :mode eldoc-mode
                       :docfn eldoc-documentation-function
                       :major major-mode))
          (sig "foo,,func(arg1 int, arg2 string) (int, error)")
          (first
           (progn
             (setq ge434-test-gocode-plan (list sig))
             (goto-char (point-min))
             (re-search-forward "foo(1")
             (goto-char (match-beginning 0))
             (re-search-forward "(")
             (ge434-test-doc)))
          (second
           (progn
             (setq ge434-test-gocode-plan (list sig))
             (re-search-forward ",")
             (ge434-test-doc))))
     (list :setup setup
           :first first
           :second second
           :calls (nreverse ge434-test-gocode-ledger)))))
"####,
        expect![[
            r#"OK (:source (:tree "99067a37568b83720e5c91c96f8ecbebed5ecd20" :manifest (("go-eldoc-pkg.el" . "6b1dd121ad4c597109545f58fee37d3f437a655310c8dd2cbd652621d41821bb") ("go-eldoc.el" . "5e1fce7fd467d0d2a85ff6072c0f600de588122fbec9674c97e45b72412f4bc9")) :feature t :version "20170305.1427") :result (:setup (:mode t :docfn go-eldoc--documentation-function :major go-mode) :first (:text "foo: (arg1 int, arg2 string) (int, error)" :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "foo") (:from 6 :to 14 :face eldoc-highlight-function-argument :part "arg1 int"))) :second (:text "foo: (arg1 int, arg2 string) (int, error)" :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "foo") (:from 16 :to 27 :face eldoc-highlight-function-argument :part "arg2 string"))) :calls ((:program "gocode" :args ("-in=text" "-f=emacs" "autocomplete" "[SANDBOX]/call.go" "c111") :file "[SANDBOX]/call.go") (:program "gocode" :args ("-in=text" "-f=emacs" "autocomplete" "[SANDBOX]/call.go" "c111") :file "[SANDBOX]/call.go"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn documents_builtins_make_and_variadic_without_gocode() -> ParityBatchCase {
    ParityBatchCase::value(
        "documents_builtins_make_and_variadic_without_gocode",
        r####"
(ge434-test-run
 (lambda (root)
   (ge434-test-visit
    root "builtin.go"
    "package main

func main() {
        n := len(xs)
        a := make([]string, 1)
        b := make([]string, 1, 2)
        c := append(dst, item)
}
")
   (setq ge434-test-gocode-plan '("" "" "" ""))
   (list :len
         (progn
           (goto-char (point-min))
           (re-search-forward "len(")
           (ge434-test-doc))
         :make-two
         (progn
           (re-search-forward "make(\\[\\]string, 1)")
           (goto-char (match-beginning 0))
           (re-search-forward ",")
           (ge434-test-doc))
         :make-three
         (progn
           (re-search-forward "make(\\[\\]string, 1, 2)")
           (goto-char (match-beginning 0))
           (re-search-forward ",")
           (re-search-forward ",")
           (ge434-test-doc))
         :append
         (progn
           (re-search-forward "append(")
           (re-search-forward ",")
           (ge434-test-doc)))))
"####,
        expect![[
            r#"OK (:source (:tree "99067a37568b83720e5c91c96f8ecbebed5ecd20" :manifest (("go-eldoc-pkg.el" . "6b1dd121ad4c597109545f58fee37d3f437a655310c8dd2cbd652621d41821bb") ("go-eldoc.el" . "5e1fce7fd467d0d2a85ff6072c0f600de588122fbec9674c97e45b72412f4bc9")) :feature t :version "20170305.1427") :result (:len (:text "len: (v Type) int" :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "len") (:from 6 :to 12 :face eldoc-highlight-function-argument :part "v Type"))) :make-two (:text "make: ([]string, size IntegerType) []string" :faces ((:from 0 :to 4 :face font-lock-function-name-face :part "make") (:from 17 :to 33 :face eldoc-highlight-function-argument :part "size IntegerType"))) :make-three (:text "make: ([]string, size IntegerType, capacity IntegerType) []string" :faces ((:from 0 :to 4 :face font-lock-function-name-face :part "make") (:from 35 :to 55 :face eldoc-highlight-function-argument :part "capacity IntegerType"))) :append (:text "append: (slice []Type, elems ...Type) []Type" :faces ((:from 0 :to 6 :face font-lock-function-name-face :part "append") (:from 23 :to 36 :face eldoc-highlight-function-argument :part "elems ...Type")))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn documents_variables_packages_and_assignment_returns() -> ParityBatchCase {
    ParityBatchCase::value(
        "documents_variables_packages_and_assignment_returns",
        r####"
(ge434-test-run
 (lambda (root)
   (ge434-test-visit
    root "types.go"
    "package main

import \"fmt\"

func foo(arg int) (int, error) {
        return 0, nil
}

func main() {
        n, err := foo(1)
        fmt.Println(n)
        值 := err
        _ = 值
}
")
   (list :pkg
         (progn
           (setq ge434-test-gocode-plan '("fmt,,package"))
           (ge434-test-at "fmt\\."))
         :assign
         (progn
           (setq ge434-test-gocode-plan '("foo,,func(arg int) (int, error)"))
           (ge434-test-at "\\bn\\b"))
         :err
         (progn
           (setq ge434-test-gocode-plan '("foo,,func(arg int) (int, error)"))
           (ge434-test-at "err :="))
         :unicode
         (progn
           (setq ge434-test-gocode-plan '("值,,var error"))
           (ge434-test-at "值 :=")))))
"####,
        expect![[
            r#"OK (:source (:tree "99067a37568b83720e5c91c96f8ecbebed5ecd20" :manifest (("go-eldoc-pkg.el" . "6b1dd121ad4c597109545f58fee37d3f437a655310c8dd2cbd652621d41821bb") ("go-eldoc.el" . "5e1fce7fd467d0d2a85ff6072c0f600de588122fbec9674c97e45b72412f4bc9")) :feature t :version "20170305.1427") :result (:pkg (:text "fmt: package" :faces ((:from 0 :to 3 :face font-lock-variable-name-face :part "fmt"))) :assign (:text "foo: (arg int) (int, error)" :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "foo") (:from 16 :to 19 :face eldoc-highlight-function-argument :part "int"))) :err (:text "foo: (arg int) (int, error)" :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "foo") (:from 21 :to 26 :face eldoc-highlight-function-argument :part "error"))) :unicode (:text "值: error" :faces ((:from 0 :to 1 :face font-lock-variable-name-face :part "值")))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn recovers_from_missing_gocode_and_non_calls() -> ParityBatchCase {
    ParityBatchCase::value(
        "recovers_from_missing_gocode_and_non_calls",
        r####"
(ge434-test-run
 (lambda (root)
   (ge434-test-visit
    root "edge.go"
    "package main

import \"net/http\"

const USERS = []string{
        \"foo\",
}

func main() {
        // foo(
        http.HandleFunc(\"/\", func(w http.ResponseWriter, r *http.Request) {
                user := USERS[i]
        })
        foo(1)
}
")
   (let* ((comment
           (progn
             (setq ge434-test-gocode-plan '("foo,,func(arg int)"))
             (goto-char (point-min))
             (re-search-forward "// foo(")
             (ge434-test-doc)))
          (missing
           (progn
             (setq ge434-test-gocode-plan :missing)
             (goto-char (point-min))
             (re-search-forward "foo(1)")
             (goto-char (match-beginning 0))
             (re-search-forward "(")
             (ge434-test-condition #'ge434-test-doc)))
          (index
           (progn
             (setq ge434-test-gocode-plan '("USERS,,func(i int)"))
             (goto-char (point-min))
             (re-search-forward "\\[i\\]")
             (goto-char (match-beginning 0))
             (forward-char 1)
             (ge434-test-doc)))
          (recovered
           (progn
             (setq ge434-test-gocode-plan
                   '("foo,,func(arg int)"))
             (goto-char (point-min))
             (re-search-forward "foo(1)")
             (goto-char (match-beginning 0))
             (re-search-forward "(")
             (ge434-test-doc))))
     (list :comment comment
           :missing missing
           :index index
           :recovered recovered))))
"####,
        expect![[
            r#"OK (:source (:tree "99067a37568b83720e5c91c96f8ecbebed5ecd20" :manifest (("go-eldoc-pkg.el" . "6b1dd121ad4c597109545f58fee37d3f437a655310c8dd2cbd652621d41821bb") ("go-eldoc.el" . "5e1fce7fd467d0d2a85ff6072c0f600de588122fbec9674c97e45b72412f4bc9")) :feature t :version "20170305.1427") :result (:comment nil :missing (:error error :data ("Searching for program: no such file or directory, gocode") :message "Searching for program: no such file or directory, gocode") :index nil :recovered (:text "foo: (arg int) " :faces ((:from 0 :to 3 :face font-lock-function-name-face :part "foo") (:from 6 :to 13 :face eldoc-highlight-function-argument :part "arg int")))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn go_eldoc_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        setup_enables_eldoc_and_documents_gocode_functions(),
        documents_builtins_make_and_variadic_without_gocode(),
        documents_variables_packages_and_assignment_returns(),
        recovers_from_missing_gocode_and_non_calls(),
    ];
    assert_oracle_batch_cases(oracle(), "go-eldoc-rank434", "go_eldoc_parity", &cases);
}
