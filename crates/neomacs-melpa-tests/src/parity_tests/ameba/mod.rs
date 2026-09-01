use std::time::Duration;

use crate::{AMEBA_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMEBA_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// ameba.el is a thin front end to the Crystal linter: it works out the project
/// root, builds `ameba --format flycheck PATH', and hands that to
/// `compilation-start' in `compilation-mode'.  What a user gets is a
/// `*Ameba PATH*' buffer whose lines `next-error' can walk into the offending
/// source, so that is what these workflows assert - the command the tool
/// receives, the buffer it produces, and where the diagnostics navigate to.
///
/// The linter itself is the environmental boundary and is stood in for, but the
/// output it replays is not invented.  `ameba-test-greeter-source' below was
/// linted by **real ameba 1.6.4** (`nix shell nixpkgs#ameba`) inside a real
/// Crystal shard, and `ameba-test-recorded-diagnostics' is what that run wrote,
/// from ameba's own `--format flycheck' serialiser rather than from its
/// documentation - rule names, severity letter, ordering and 1-based
/// line:column all as the tool emits them, including that it reports line 14
/// before line 6.  The stand-in substitutes the sandbox path for the recording's
/// project root and exits 1, which is the status the real run exited with.  The
/// argv it records is therefore the load-bearing assertion: it is what proves
/// the package asked the linter for the right thing.
///
/// Recorded verbatim from:
///   ameba --format flycheck src/greeter.cr   (ameba 1.6.4, Crystal 1.18.2)
///
/// `ameba-test-compilation-text' removes the two things in a compilation buffer
/// that are not the package's doing: the wall-clock stamps in the header and
/// footer, and the sandbox path, in both its plain and `abbreviate-file-name'
/// forms - the header writes the abbreviated one.
const AMEBA_TEST_PRELUDE: &str = r##"(require 'cl-lib)
(require 'compile)

(defconst ameba-test-greeter-source
  "class Greeter\n  def initialize(@name : String)\n  end\n\n  def greet\n    unused = \"not used anywhere\"\n    result = begin\n      \"Hello, #{@name}!\"\n    end\n    result\n  end\n\n  def shout\n    if true\n      \"HELLO\"\n    else\n      \"hello\"\n    end\n  end\nend\n"
  "A Crystal class with exactly the two faults ameba 1.6.4 reports below.")

(defconst ameba-test-util-source
  "module Util\n  def self.twice(value)\n    x = value\n    value + value\n  end\nend\n"
  "A second source file, faulty in a third way.")

(defconst ameba-test-vendored-source
  "module Vendored\n  def self.noop\n    dead = 1\n  end\nend\n"
  "A file below lib/, which the project check asks ameba to exclude.")

(defun ameba-test-file-diagnostics (file)
  "What real ameba 1.6.4 printed for FILE, in its own order."
  (concat file ":14:5: W: [Lint/LiteralInCondition] Literal value found in conditional\n"
          file ":6:5: W: [Lint/UselessAssign] Useless assignment to variable `unused`\n"))

(defun ameba-test-project-diagnostics (root)
  "What real ameba 1.6.4 printed for the project with lib/ excluded."
  (concat (ameba-test-file-diagnostics (concat root "src/greeter.cr"))
          root "src/util.cr:3:5: W: [Lint/UselessAssign] Useless assignment to variable `x`\n"))

(defun ameba-test-project (name &optional marker)
  "Build a Crystal shard called NAME in the sandbox, and return its root.
MARKER, when given, is an extra project-root file to create."
  (let ((root (file-name-as-directory
               (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory (expand-file-name "src" root) t)
    (make-directory (expand-file-name "lib" root) t)
    (write-region "name: greeter\nversion: 0.1.0\n" nil
                  (expand-file-name "shard.yml" root) nil 'silent)
    (when marker
      (write-region "" nil (expand-file-name marker root) nil 'silent))
    (write-region ameba-test-greeter-source nil
                  (expand-file-name "src/greeter.cr" root) nil 'silent)
    (write-region ameba-test-util-source nil
                  (expand-file-name "src/util.cr" root) nil 'silent)
    (write-region ameba-test-vendored-source nil
                  (expand-file-name "lib/vendored.cr" root) nil 'silent)
    root))

(defun ameba-test-install-linter (root output status)
  "Install a stand-in `ameba' for ROOT replaying OUTPUT and exiting STATUS."
  (let* ((bin (expand-file-name "bin" root))
         (program (expand-file-name "ameba" bin)))
    (make-directory bin t)
    (write-region
     (concat "#!/bin/sh\n"
             "printf '<%s>\\n' \"$@\" > \"$AMEBA_TEST_LOG\"\n"
             "printf '%s' \"$AMEBA_TEST_OUTPUT\"\n"
             "exit ${AMEBA_TEST_STATUS:-0}\n")
     nil program nil 'silent)
    (set-file-modes program #o755)
    (setenv "AMEBA_TEST_LOG" (expand-file-name "ameba.log" root))
    (setenv "AMEBA_TEST_OUTPUT" output)
    (setenv "AMEBA_TEST_STATUS" (number-to-string status))
    ;; `executable-find' consults `exec-path', not PATH.
    (add-to-list 'exec-path bin)
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    program))

(defun ameba-test-arguments ()
  "The argv the stand-in linter recorded, one string per argument."
  (with-temp-buffer
    (insert-file-contents (getenv "AMEBA_TEST_LOG"))
    (let (arguments)
      (goto-char (point-min))
      (while (re-search-forward "^<\\(.*\\)>$" nil t)
        (push (match-string-no-properties 1) arguments))
      (nreverse arguments))))

(defun ameba-test-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output rather than a guess about it:
Emacs drains a dying process's remaining reads before it runs the
sentinel, the sentinel is what calls `compilation-handle-exit', and that
function marks the text it writes with a `compilation-handle-exit' text
property (lisp/progmodes/compile.el:2630).  The property cannot appear
until every byte the linter wrote has been through `compilation-filter'."
  (and (buffer-live-p (get-buffer buffer))
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun ameba-test-wait (buffer)
  "Wait until BUFFER holds all of its compilation's output, or signal.
The linter's process dying is not that condition: it can be gone with
reads still queued, and a buffer pinned at that moment records however
much of its output the kernel happened to have delivered -- a fact about
scheduling rather than about either editor, which is the defect
DIVERGENCES.md 133 removed from the `rg' suite.  Signalling rather than
returning keeps a future edit that reintroduces the race from being
recorded as a snapshot."
  (let ((limit 1200))
    (while (and (> limit 0) (not (ameba-test-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq limit (1- limit)))
    (unless (ameba-test-complete-p buffer)
      (error "ameba-test-wait: %s never reached `compilation-handle-exit'; \
its text records only as much of the linter's output as had been read"
             buffer))
    :finished))

(defun ameba-test-relative (text root)
  "TEXT with ROOT written as `PROJECT/', in plain and abbreviated spellings.
A marker rather than an empty string, so that an argument which is only the
project root stays visible as one instead of collapsing to \"\"."
  (replace-regexp-in-string
   (regexp-quote (abbreviate-file-name root)) "PROJECT/"
   (replace-regexp-in-string (regexp-quote root) "PROJECT/" text)))

(defun ameba-test-compilation-text (buffer root)
  "BUFFER's text with ROOT and the wall-clock stamps taken out."
  (with-current-buffer buffer
    (ameba-test-relative
     (replace-regexp-in-string
      "duration [0-9.]+ s" "duration [DURATION]"
      (replace-regexp-in-string
       "at [A-Z][a-z][a-z] [A-Z][a-z][a-z] +[0-9]+ [0-9][0-9]:[0-9][0-9]:[0-9][0-9]"
       "at [TIME]"
       (buffer-substring-no-properties (point-min) (point-max))))
     root)))

(defun ameba-test-jump (compilation-buffer)
  "Walk to the next diagnostic and follow it, the way `next-error' does."
  (with-current-buffer compilation-buffer
    (compilation-next-error 1)
    (compile-goto-error)
    (list (buffer-name)
          (line-number-at-pos)
          (current-column)
          (buffer-substring-no-properties (line-beginning-position)
                                          (line-end-position)))))
"##;

fn ameba_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMEBA_MELPA_PIN, "ameba.el")
        .expect("prepare pinned Ameba source below ./tmp")
        .with_prelude(AMEBA_TEST_PRELUDE)
        .with_timeout(AMEBA_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Ameba parity test").into()
}

/// Multi-probe batch for `assert_ameba_parity` cases (2a).
pub(crate) fn assert_ameba_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ameba_oracle(), &name, "ameba_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ameba_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ameba_batch(&cases);
}

// END generated package batch tests
