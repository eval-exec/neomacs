use std::time::Duration;

use crate::{AA_EDIT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in `workflows` use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AA_EDIT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Shift_JIS art fixtures and sandbox helpers shared by the workflows.  The
/// panels are ordinary yaruo-style AA: every character below is representable
/// in Shift_JIS, so a `.mlt` file written from them is a realistic AA file
/// rather than a UTF-8 file with a Japanese name.
const AA_EDIT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst aa-edit-test-panels
  '("　　　（´д｀）\n　＿ノ　　ヽ、＿\n"
    "やる夫「ＡＡだお」\n　　∧＿∧\n　（　´∀｀）\n"
    "おわり\n"))

(defun aa-edit-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aa-edit-test-write-file (name text)
  "Write TEXT to sandbox file NAME as Shift_JIS bytes and return its path."
  (let ((path (aa-edit-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (set-buffer-multibyte nil)
      (insert (encode-coding-string text 'japanese-shift-jis t))
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun aa-edit-test-write-mlt (name separator)
  "Write the AA panels to NAME joined by SEPARATOR as a Shift_JIS file."
  (aa-edit-test-write-file
   name
   (mapconcat #'identity aa-edit-test-panels separator)))

(defun aa-edit-test-file-bytes (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun aa-edit-test-file-sha256 (path)
  (secure-hash 'sha256 (aa-edit-test-file-bytes path)))

(defun aa-edit-test-directory-listing (name)
  (mapcar
   (lambda (path)
     (cons (file-name-nondirectory path)
           (file-attribute-size (file-attributes path))))
   (sort (directory-files (aa-edit-test-path name) t "\\`[^.]") #'string<)))
"##;

fn aa_edit_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AA_EDIT_MODE_MELPA_PIN, "aa-edit-mode.el")
        .expect("prepare pinned aa-edit-mode source below ./tmp")
        .with_prelude(AA_EDIT_MODE_TEST_PRELUDE)
        .with_timeout(AA_EDIT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aa-edit-mode parity test")
        .into()
}

/// Multi-probe batch from structured [`ParityBatchCase`] constructors.
pub(crate) fn assert_aa_edit_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aa_edit_mode_oracle(), &name, "aa-edit-mode", cases);
}

// BEGIN generated package batch tests

#[test]
fn aa_edit_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::aa_edit_mode_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aa_edit_mode_batch(&cases);
}

// END generated package batch tests
