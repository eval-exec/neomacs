use std::time::Duration;

use crate::{AES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AES_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// aes.el is a pure-Elisp AES implementation: it encrypts buffers and strings,
/// writes a self-describing header, and stores the result either base64-encoded
/// or as raw bytes.  Everything it does is local and deterministic except two
/// things -- the random initialisation vector, which makes whole-ciphertext
/// comparison meaningless by design, and the password prompt.  So the workflows
/// pin exact bytes where they are deterministic (the FIPS-197 known-answer
/// vector, and a fixed cipher block written through a coding system) and pin
/// round trips, headers and lengths everywhere else.  The only stand-in is
/// `read-passwd`, a genuine interactive boundary.
const AES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(setq make-backup-files nil create-lockfiles nil)

(defvar aes-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aes-test-path (name) (expand-file-name name aes-test-root))

(defun aes-test-bytes (path)
  "Exact bytes of PATH as a list of integers."
  (if (file-regular-p path)
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (let ((coding-system-for-read 'binary)) (insert-file-contents-literally path))
        (string-to-list (buffer-string)))
    'no-such-file))

(defun aes-test-text (path &optional coding)
  (if (file-regular-p path)
      (with-temp-buffer
        (let ((coding-system-for-read (or coding 'utf-8))) (insert-file-contents path))
        (buffer-substring-no-properties (point-min) (point-max)))
    'no-such-file))

(defun aes-test-hex (string)
  (mapconcat (lambda (c) (format "%02x" c)) (string-to-list string) ""))

(defun aes-test-unhex (hex)
  (let ((s (make-string (/ (length hex) 2) 0)) (i 0))
    (while (< i (length s))
      (aset s i (string-to-number (substring hex (* 2 i) (+ 2 (* 2 i))) 16))
      (setq i (1+ i)))
    s))

(defun aes-test-header (encrypted)
  "The plaintext header line of an encrypted blob."
  (if (string-match "\\`\\(aes-encrypted V [^\n]*\\)\n" encrypted)
      (match-string 1 encrypted)
    'no-header))
"##;

fn aes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AES_MELPA_PIN, "aes.el")
        .expect("prepare pinned aes source below ./tmp")
        .with_prelude(AES_TEST_PRELUDE)
        .with_timeout(AES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aes parity test").into()
}

/// Multi-probe batch for `assert_aes_parity` cases (2a).
pub(crate) fn assert_aes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aes_oracle(), &name, "aes_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aes_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aes_batch(&cases);
}

// END generated package batch tests
