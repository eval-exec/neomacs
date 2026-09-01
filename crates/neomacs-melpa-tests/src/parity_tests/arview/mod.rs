use std::time::Duration;

use crate::{ARVIEW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARVIEW_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ARVIEW_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun arview-test-path (filename)
  (expand-file-name
   filename
   (getenv
    "NEOMACS_TEST_SANDBOX_ROOT")))

(defun arview-test-write-bytes (path bytes)
  (make-directory
   (file-name-directory path)
   t)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert bytes)
    (write-region
     (point-min)
     (point-max)
     path
     nil
     'silent))
  path)

(defun arview-test-file-sha256 (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun arview-test-create-project-tar
    (archive-name nested-member)
  (let* ((source
          (arview-test-path
           "release-source"))
         (archive
          (arview-test-path
           archive-name))
         (log
          (get-buffer-create
           " *arview-test-tar-log*")))
    (make-directory
     (file-name-directory
      (expand-file-name
       nested-member
       source))
     t)
    (arview-test-write-bytes
     (expand-file-name
      "README.md"
      source)
     (encode-coding-string
      "# Widget release\nInstall from build/widget.bin.\n"
      'utf-8-unix
      t))
    (arview-test-write-bytes
     (expand-file-name
      nested-member
      source)
     (encode-coding-string
      "endpoint=https://example.invalid/api\nretries=3\n"
      'utf-8-unix
      t))
    (arview-test-write-bytes
     (expand-file-name
      "build/widget.bin"
      source)
     (unibyte-string
      0 1 2 10 13 31 32 127
      128 129 191 200 254 255))
    (with-current-buffer log
      (erase-buffer))
    (let ((exit
           (process-file
            "tar"
            nil
            log
            nil
            "-cf"
            archive
            "-C"
            source
            ".")))
      (unless (zerop exit)
        (error
         "Fixture tar failed: %s"
         (with-current-buffer log
           (buffer-string)))))
    archive))

(defun arview-test-manifest (directory)
  (mapcar
   (lambda (path)
     (list
      (file-relative-name
       path
       directory)
      (file-attribute-size
       (file-attributes path))
      (arview-test-file-sha256 path)))
   (sort
    (directory-files-recursively
     directory
     ".*"
     nil
     nil)
    #'string<)))
"##;

fn arview_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARVIEW_MELPA_PIN, source_file)
        .expect("prepare pinned arview source below ./tmp")
        .with_prelude(ARVIEW_TEST_PRELUDE)
        .with_timeout(ARVIEW_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed arview parity test").into()
}

/// Multi-probe batch for `assert_arview_autoload_parity` cases (2a).
pub(crate) fn assert_arview_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arview_oracle("arview-autoloads.el"),
        &name,
        "arview_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_arview_parity` cases (2a).
pub(crate) fn assert_arview_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(arview_oracle("arview.el"), &name, "arview_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn arview_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_arview_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arview_autoload_batch(&cases);
}

#[test]
fn arview_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_arview_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arview_batch(&cases);
}

// END generated package batch tests
