use std::time::Duration;

use crate::{AST_GREP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backends;
mod candidates;
mod commands;
mod outline;
mod registry;
mod rewrite;
mod sync;
mod workflows;

const AST_GREP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AST_GREP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defun ast-grep-test-path (filename)
  (expand-file-name filename (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ast-grep-test-write-file (filename content)
  (let ((path (ast-grep-test-path filename)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert content))
    path))

(defun ast-grep-test-read-file (filename)
  (with-temp-buffer
    (insert-file-contents-literally filename)
    (buffer-string)))

(defun ast-grep-test-make-executable (name body)
  (let ((path (ast-grep-test-write-file
               (concat "bin/" name)
               (concat "#!/bin/sh\nset -eu\n" body "\n"))))
    (set-file-modes path #o755)
    path))

(defun ast-grep-test-error-data (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error (list :error (car error-data) (cdr error-data)))))

(defun ast-grep-test-match-summary (candidate)
  (let ((match (ast-grep--candidate-match candidate)))
    (and match
         (list
          (plist-get match :file)
          (plist-get match :start-line)
          (plist-get match :start-column)
          (plist-get match :end-line)
          (plist-get match :end-column)
          (plist-get match :text)
          (plist-get match :replacement)))))

(defun ast-grep-test-kill-file-buffer (file)
  (when-let ((buffer (find-buffer-visiting file)))
    (with-current-buffer buffer
      (set-buffer-modified-p nil))
    (kill-buffer buffer)))


;;; --- Real ast-grep 0.40.0 replay -------------------------------------------

(defvar ast-grep-test-records
  (file-name-as-directory
   (expand-file-name "ast-grep-records" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ast-grep-test-calls
  (expand-file-name "ast-grep-calls.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar ast-grep-test-misses
  (expand-file-name "ast-grep-misses.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst ast-grep-test-recordings
  '(
    (("outline" "--json=stream" "@@PROJECT@@/src/app.js") 2 "" "error: unrecognized subcommand 'outline'\n\nUsage: ast-grep [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.\n")
    (("run" "--pattern=console.log" "--json=stream" "@@PROJECT@@") 0 "{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":0,\"end\":11},\"start\":{\"line\":0,\"column\":0},\"end\":{\"line\":0,\"column\":11}},\"file\":\"@@PROJECT@@/src/other.js\",\"lines\":\"console.log(\\\"second file\\\");\",\"charCount\":{\"leading\":0,\"trailing\":16},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":23,\"end\":34},\"start\":{\"line\":0,\"column\":23},\"end\":{\"line\":0,\"column\":34}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"const greet = (who) => console.log(\\\"hi \\\" + who);\",\"charCount\":{\"leading\":23,\"trailing\":14},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":49,\"end\":60},\"start\":{\"line\":1,\"column\":0},\"end\":{\"line\":1,\"column\":11}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"console.log(\\\"starting\\\");\",\"charCount\":{\"leading\":0,\"trailing\":13},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":94,\"end\":105},\"start\":{\"line\":3,\"column\":2},\"end\":{\"line\":3,\"column\":13}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"  console.log(\\\"ready\\\");\",\"charCount\":{\"leading\":2,\"trailing\":10},\"language\":\"JavaScript\"}\n" "")
    (("run" "--pattern=console.log" "--rewrite=logger.info" "--json=stream" "@@PROJECT@@") 0 "{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":23,\"end\":34},\"start\":{\"line\":0,\"column\":23},\"end\":{\"line\":0,\"column\":34}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"const greet = (who) => console.log(\\\"hi \\\" + who);\",\"charCount\":{\"leading\":23,\"trailing\":14},\"replacement\":\"logger.info\",\"replacementOffsets\":{\"start\":23,\"end\":34},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":49,\"end\":60},\"start\":{\"line\":1,\"column\":0},\"end\":{\"line\":1,\"column\":11}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"console.log(\\\"starting\\\");\",\"charCount\":{\"leading\":0,\"trailing\":13},\"replacement\":\"logger.info\",\"replacementOffsets\":{\"start\":49,\"end\":60},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":94,\"end\":105},\"start\":{\"line\":3,\"column\":2},\"end\":{\"line\":3,\"column\":13}},\"file\":\"@@PROJECT@@/src/app.js\",\"lines\":\"  console.log(\\\"ready\\\");\",\"charCount\":{\"leading\":2,\"trailing\":10},\"replacement\":\"logger.info\",\"replacementOffsets\":{\"start\":94,\"end\":105},\"language\":\"JavaScript\"}\n{\"text\":\"console.log\",\"range\":{\"byteOffset\":{\"start\":0,\"end\":11},\"start\":{\"line\":0,\"column\":0},\"end\":{\"line\":0,\"column\":11}},\"file\":\"@@PROJECT@@/src/other.js\",\"lines\":\"console.log(\\\"second file\\\");\",\"charCount\":{\"leading\":0,\"trailing\":16},\"replacement\":\"logger.info\",\"replacementOffsets\":{\"start\":0,\"end\":11},\"language\":\"JavaScript\"}\n" "")))

(defun ast-grep-test-key (arguments)
  "Return the record key for ARGUMENTS.

Any argument holding a `/' is reduced to its base name, because the package
passes absolute paths -- the project directory to `run', the visited file to
`outline' -- and those differ between the machine a recording was made on and
the per-case sandbox.  Must agree exactly with the shell stand-in."
  (mapconcat
   (lambda (argument)
     (let ((base (if (string-match-p "/" argument)
                     ;; `directory-file-name' first: the package spells the
                     ;; search root with a trailing slash, whose base name is
                     ;; the empty string, so without this every directory
                     ;; argument keys the same and none of them match.
                     (file-name-nondirectory (directory-file-name argument))
                   argument)))
       (concat "~" (replace-regexp-in-string "[^A-Za-z0-9._-]" "_" base))))
   arguments ""))

(defconst ast-grep-test-stand-in
  (string-join
   (list
    "#!/bin/sh"
    "# Replay stand-in for ast-grep 0.40.0.  Every reply was recorded from the"
    "# real binary; this only looks one up and refuses to invent an answer."
    "key=\"\""
    "for a in \"$@\"; do"
    "  case \"$a\" in */) a=${a%/} ;; esac"
    "  case \"$a\" in */*) a=${a##*/} ;; esac"
    "  key=\"$key~$(printf '%s' \"$a\" | tr -c 'A-Za-z0-9._-' '_')\""
    "done"
    "printf '%s\\n' \"$(printf '%s|' \"$@\" | tr '\\n' '~')\" >> \"$AST_GREP_TEST_CALLS\""
    "d=\"$AST_GREP_TEST_RECORDS/$key\""
    "if [ ! -f \"$d/rc\" ]; then"
    "  printf '%s\\n' \"$(printf '%s|' \"$@\" | tr '\\n' '~')\" >> \"$AST_GREP_TEST_MISSES\""
    "  printf 'UNRECORDED ast-grep invocation: %s\\n' \"$*\" >&2"
    "  exit 99"
    "fi"
    "cat \"$d/out\""
    "cat \"$d/err\" >&2"
    "exit \"$(cat \"$d/rc\")\""
    "")
   "\n"))

(defun ast-grep-test-install (project)
  "Install the recorded ast-grep stand-in on `exec-path', bound to PROJECT.

Recordings hold the recording machine's project path under the token
`@@PROJECT@@'; it is substituted for PROJECT here, so the JSON the package
parses names files that really exist in this sandbox."
  (let ((installed nil)
        (bin (expand-file-name "bin" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (root (directory-file-name (expand-file-name project))))
    (dolist (recording ast-grep-test-recordings)
      (let* ((substitute (lambda (text)
                           (replace-regexp-in-string
                            "@@PROJECT@@" root text t t)))
             ;; Materialise the argument vector before keying it.  The token
             ;; stands in for the recording machine's project path in the
             ;; arguments as well as in the output, and a key computed from the
             ;; token spells `__PROJECT__' while the replayed call spells the
             ;; real directory -- every lookup misses.
             (arguments (mapcar substitute (nth 0 recording)))
             (key (ast-grep-test-key arguments))
             (path (expand-file-name key ast-grep-test-records)))
        (when (member path installed)
          (error "Record key collision for %S" arguments))
        (push path installed)
        (make-directory path t)
        (ast-grep-test-write-raw (expand-file-name "rc" path)
                                 (format "%d\n" (nth 1 recording)))
        (ast-grep-test-write-raw (expand-file-name "out" path)
                                 (funcall substitute (nth 2 recording)))
        (ast-grep-test-write-raw (expand-file-name "err" path)
                                 (funcall substitute (nth 3 recording)))))
    (setenv "AST_GREP_TEST_RECORDS" (directory-file-name ast-grep-test-records))
    (setenv "AST_GREP_TEST_CALLS" ast-grep-test-calls)
    (setenv "AST_GREP_TEST_MISSES" ast-grep-test-misses)
    (make-directory bin t)
    (let ((path (expand-file-name "ast-grep" bin)))
      (ast-grep-test-write-raw path ast-grep-test-stand-in)
      (set-file-modes path #o755))
    (setq exec-path (cons bin exec-path))
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    (length installed)))

(defun ast-grep-test-write-raw (path content)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ast-grep-test-project ()
  "Create the recorded JavaScript project below the sandbox and return it."
  (let ((root (file-name-as-directory
               (expand-file-name "proj" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (ast-grep-test-write-raw
     (expand-file-name "src/app.js" root)
     "const greet = (who) => console.log(\"hi \" + who);\nconsole.log(\"starting\");\nfunction main() {\n  console.log(\"ready\");\n  return greet(\"world\");\n}\n")
    (ast-grep-test-write-raw
     (expand-file-name "src/other.js" root)
     "console.log(\"second file\");\n")
    root))

(defun ast-grep-test-calls-made ()
  (if (not (file-exists-p ast-grep-test-calls))
      'ast-grep-was-never-run
    (with-temp-buffer
      (insert-file-contents ast-grep-test-calls)
      (mapcar (lambda (line)
                (replace-regexp-in-string
                 (regexp-quote (directory-file-name
                                (expand-file-name
                                 "proj" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 "<project>" line t t))
              (split-string (buffer-string) "\n" t)))))

(defun ast-grep-test-unrecorded ()
  "Invocations the stand-in had no recording for.

Asserted empty by every workflow: ast-grep exits 0 with no output when a
pattern simply does not match, so a stand-in answering nothing is
indistinguishable from a successful search that found nothing."
  (if (not (file-exists-p ast-grep-test-misses))
      nil
    (with-temp-buffer
      (insert-file-contents ast-grep-test-misses)
      (split-string (buffer-string) "\n" t))))
"##;

fn ast_grep_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AST_GREP_MELPA_PIN, source_file)
        .expect("prepare pinned ast-grep source below ./tmp")
        .with_prelude(AST_GREP_TEST_PRELUDE)
        .with_timeout(AST_GREP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ast-grep parity test")
        .into()
}

/// Multi-probe batch for `assert_ast_grep_parity` cases (2a).
pub(crate) fn assert_ast_grep_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ast_grep_oracle("ast-grep.el"),
        &name,
        "ast_grep_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_ast_grep_consult_parity` cases (2a).
pub(crate) fn assert_ast_grep_consult_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ast_grep_oracle("ast-grep-consult.el"),
        &name,
        "ast_grep_consult_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_ast_grep_helm_parity` cases (2a).
pub(crate) fn assert_ast_grep_helm_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ast_grep_oracle("ast-grep-helm.el"),
        &name,
        "ast_grep_helm_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_ast_grep_ivy_parity` cases (2a).
pub(crate) fn assert_ast_grep_ivy_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ast_grep_oracle("ast-grep-ivy.el"),
        &name,
        "ast_grep_ivy_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ast_grep_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_ast_grep_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        commands::commands_public_surface_batch_cases(),
        outline::outline_public_surface_batch_cases(),
        registry::registry_ast_grep_batch_cases(),
        rewrite::rewrite_public_surface_batch_cases(),
        sync::sync_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ast_grep_batch(&cases);
}

#[test]
fn ast_grep_consult_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_ast_grep_consult_batch_cases(),
        registry::registry_ast_grep_consult_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ast_grep_consult_batch(&cases);
}

#[test]
fn ast_grep_helm_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_ast_grep_helm_batch_cases(),
        registry::registry_ast_grep_helm_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ast_grep_helm_batch(&cases);
}

#[test]
fn ast_grep_ivy_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_ast_grep_ivy_batch_cases(),
        registry::registry_ast_grep_ivy_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ast_grep_ivy_batch(&cases);
}

// END generated package batch tests
