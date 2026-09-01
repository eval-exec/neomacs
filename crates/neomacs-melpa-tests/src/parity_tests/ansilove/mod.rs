use std::time::Duration;

use crate::{ANSILOVE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod practical;

const ANSILOVE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ANSILOVE_TEST_PRELUDE: &str = r##"
(defun neomacs-ansilove-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when
          (and file
               (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when (get-buffer "*Ansilove-Output*")
    (kill-buffer "*Ansilove-Output*"))
  (when (file-exists-p root)
    (delete-directory root t)))

(defun neomacs-ansilove-test-write-file (file contents)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun neomacs-ansilove-test-read-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (string-to-list (buffer-string))))

(defun neomacs-ansilove-test-file-summary (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (list
     (buffer-size)
     (string-to-list
      (buffer-substring-no-properties
       (point-min)
       (+ (point-min) 8))))))

(defun neomacs-ansilove-test-output-name (file root)
  (let ((relative (file-relative-name file root)))
    (setq relative
          (replace-regexp-in-string
           "ansilove_[0-9]+\\.png\\'"
           "ansilove_<id>.png"
           relative))
    (replace-regexp-in-string
     "\\.\\#ansilove_[0-9]+\\.txt\\'"
     ".#ansilove_<id>.txt"
     relative)))

(defun neomacs-ansilove-test-write-converter (root)
  (let ((converter
         (expand-file-name "bin/ansilove-fixture" root)))
    (neomacs-ansilove-test-write-file
     converter
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "if [ \"$#\" -ne 3 ] || [ \"$1\" != \"-o\" ]; then\n"
      "  printf 'usage: ansilove-fixture -o OUTPUT INPUT\\n' >&2\n"
      "  exit 64\n"
      "fi\n"
      "output=$2\n"
      "input=$3\n"
      "if [ ! -r \"$input\" ]; then\n"
      "  printf 'unreadable input: %s\\n' \"$input\" >&2\n"
      "  exit 66\n"
      "fi\n"
      "bytes=$(wc -c < \"$input\" | tr -d ' ')\n"
      "printf 'ansilove-fixture: converted %s bytes\\n' \"$bytes\"\n"
      "printf '%s' "
      "'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=' "
      "| base64 -d > \"$output\"\n"))
    (set-file-modes converter #o755)
    converter))
"##;

fn ansilove_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANSILOVE_MELPA_PIN, "ansilove.el")
        .expect("prepare pinned ansilove source below ./tmp")
        .with_prelude(ANSILOVE_TEST_PRELUDE)
        .with_timeout(ANSILOVE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ansilove parity test")
        .into()
}

/// Multi-probe batch for `assert_ansilove_parity` cases (2a).
pub(crate) fn assert_ansilove_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ansilove_oracle(), &name, "ansilove_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ansilove_package_batch() {
    let cases: Vec<ParityBatchCase> = [practical::practical_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ansilove_batch(&cases);
}

// END generated package batch tests
