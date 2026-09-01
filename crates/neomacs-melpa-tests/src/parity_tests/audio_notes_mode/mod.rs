use std::time::Duration;

use crate::{AUDIO_NOTES_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod filesystem;
mod lifecycle;
mod playback;
mod process;
mod registry;

const AUDIO_NOTES_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUDIO_NOTES_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun audio-notes-test-error
    (thunk)
  (condition-case error
      (list
       :ok
       (funcall thunk))
    (error
     (list
      :signal
      (car error)
      (cdr error)))))

(defun audio-notes-test-warning
    (thunk)
  (let (warnings)
    (cl-letf
        (((symbol-function 'display-warning)
          (lambda
            (type message &optional level buffer-name)
            (push
             (list type message level buffer-name)
             warnings))))
      (list
       (funcall thunk)
       (nreverse warnings)))))

(defun audio-notes-test-directory
    (name)
  (let ((directory
         (expand-file-name
          (concat name "/")
          default-directory)))
    (make-directory directory t)
    directory))

(defun audio-notes-test-write
    (directory name contents)
  (let ((path
         (expand-file-name
          name
          directory)))
    (with-temp-file path
      (insert contents))
    path))

(defun audio-notes-test-face-property
    (string)
  (list
   (substring-no-properties string)
   (get-text-property 0 'face string)))
"##;

fn audio_notes_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUDIO_NOTES_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned audio-notes-mode source below ./tmp")
        .with_prelude(AUDIO_NOTES_MODE_TEST_PRELUDE)
        .with_timeout(AUDIO_NOTES_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed audio-notes-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_audio_notes_mode_autoload_parity` cases (2a).
pub(crate) fn assert_audio_notes_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        audio_notes_mode_oracle("audio-notes-mode-autoloads.el"),
        &name,
        "audio_notes_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_audio_notes_mode_parity` cases (2a).
pub(crate) fn assert_audio_notes_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        audio_notes_mode_oracle("audio-notes-mode.el"),
        &name,
        "audio_notes_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn audio_notes_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_audio_notes_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_audio_notes_mode_autoload_batch(&cases);
}

#[test]
fn audio_notes_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        filesystem::filesystem_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        playback::playback_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        registry::registry_audio_notes_mode_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_audio_notes_mode_batch(&cases);
}

// END generated package batch tests
