use std::time::Duration;

use crate::{ABC_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ABC_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A real three-tune ABC session book plus sandbox helpers.  The tunes are
/// ordinary traditional ABC: duplicated `X:` reference numbers, an ABC comment
/// line, chord symbols and bar lines, so renumbering, navigation, chord
/// extraction and bar alignment all have something real to work on.
const ABC_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst abc-test-tunebook
  (concat
   "X:7\n"
   "T:Si Beag, Si Mor\n"
   "C:Turlough O'Carolan\n"
   "M:3/4\n"
   "L:1/8\n"
   "Q:1/4=120\n"
   "K:D\n"
   "|:A2|d3 e f2|e3 d B2|A3 B A2|F4 A2|\n"
   "%% a comment line\n"
   "X:7\n"
   "T:The Butterfly\n"
   "M:9/8\n"
   "L:1/8\n"
   "K:Em\n"
   "|:B3 AFE|B2 E E2 F|G3 AGF|GFE FED|\n"
   "X:2\n"
   "T:Planxty Irwin\n"
   "M:3/4\n"
   "L:1/8\n"
   "K:G\n"
   "D2|G3 A B2|d3 e d2|B3 A G2|E4 D2|\n"))

(defun abc-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun abc-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (abc-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun abc-test-open (name text)
  "Visit a sandbox ABC file holding TEXT and return its buffer."
  (find-file-noselect (abc-test-write name text)))

(defun abc-test-write-executable (name body)
  (let ((path (abc-test-path (concat "bin/" name))))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun abc-test-setup-tools ()
  "Install recording stand-ins for the abc2ps/abc2midi/abc2abc tools."
  (dolist (tool '("abcm2ps" "abc2midi" "abc2abc"))
    (abc-test-write-executable
     tool
     (concat "#!/bin/sh\n"
             "printf '%s\\n' \"" tool " $*\" >> \"$ABC_LOG\"\n"
             "exit 0\n")))
  (setenv "ABC_LOG" (abc-test-path "commands.log"))
  (setenv "PATH" (concat (abc-test-path "bin") path-separator (getenv "PATH"))))

(defun abc-test-commands ()
  (let ((log (abc-test-path "commands.log")))
    (if (file-exists-p log)
        (with-temp-buffer
          (insert-file-contents log)
          (split-string (buffer-string) "\n" t))
      'no-command-ran)))
"##;

fn abc_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ABC_MODE_MELPA_PIN, "abc-mode.el")
        .expect("prepare pinned abc-mode source below ./tmp")
        .with_prelude(ABC_MODE_TEST_PRELUDE)
        .with_timeout(ABC_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed abc-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_abc_mode_parity` cases (2a).
pub(crate) fn assert_abc_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(abc_mode_oracle(), &name, "abc_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn abc_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_abc_mode_batch(&cases);
}

// END generated package batch tests
