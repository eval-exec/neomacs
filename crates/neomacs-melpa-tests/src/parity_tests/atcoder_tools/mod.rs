use std::time::Duration;

use crate::{ATCODER_TOOLS_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod commands;
mod configuration;
mod metadata;
mod registry;
mod workflows;

const ATCODER_TOOLS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ATCODER_TOOLS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun atcoder-tools-test-error-data (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun atcoder-tools-test-root ()
  (let ((root
         (file-name-as-directory
          (expand-file-name
           "atcoder-tools-case"
           (getenv "TMPDIR")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun atcoder-tools-test-write-file
    (root relative-name contents)
  (let ((file
         (expand-file-name relative-name root)))
    (make-directory
     (file-name-directory file)
     t)
    (with-temp-file file
      (insert contents))
    file))

(defun atcoder-tools-test-read-file (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun atcoder-tools-test-normalize
    (value root)
  (if (stringp value)
      (let* ((absolute-root
              (directory-file-name root))
             (relative-root
              (directory-file-name
               (file-relative-name
                root
                default-directory)))
             (absolute-normalized
              (replace-regexp-in-string
               (regexp-quote absolute-root)
               "[ROOT]"
               value
               t
               t)))
        (replace-regexp-in-string
         (regexp-quote relative-root)
         "[ROOT]"
         absolute-normalized
         t
         t))
    value))

(defun atcoder-tools-test-normalize-tree
    (value root)
  (cond
   ((stringp value)
    (atcoder-tools-test-normalize
     value
     root))
   ((consp value)
    (cons
     (atcoder-tools-test-normalize-tree
      (car value)
      root)
     (atcoder-tools-test-normalize-tree
      (cdr value)
      root)))
   ((vectorp value)
    (apply
     #'vector
     (mapcar
      (lambda (item)
        (atcoder-tools-test-normalize-tree
         item
         root))
      value)))
   (t value)))

(defun atcoder-tools-test-tree (root)
  (mapcar
   (lambda (file)
     (let ((relative
            (file-relative-name file root)))
       (list
        relative
        (file-attribute-size
         (file-attributes file))
        (secure-hash
         'sha256
         (atcoder-tools-test-read-file
          file)))))
   (sort
    (directory-files-recursively
     root
     ".*"
     nil
     nil
     t)
    #'string<)))

(defun atcoder-tools-test-config-snapshot
    (config)
  (list
   (copy-tree
    (alist-get 'cmd-templates config))
   (alist-get 'remove-exec config)
   (length config)))
"##;

fn atcoder_tools_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ATCODER_TOOLS_MELPA_PIN, source_file)
        .expect("prepare pinned atcoder-tools source and dependencies below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(ATCODER_TOOLS_TEST_PRELUDE)
        .with_timeout(ATCODER_TOOLS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed atcoder-tools parity test")
        .into()
}

/// Multi-probe batch for `assert_atcoder_tools_autoload_parity` cases (2a).
pub(crate) fn assert_atcoder_tools_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        atcoder_tools_oracle("atcoder-tools-autoloads.el"),
        &name,
        "atcoder_tools_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_atcoder_tools_parity` cases (2a).
pub(crate) fn assert_atcoder_tools_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        atcoder_tools_oracle("atcoder-tools.el"),
        &name,
        "atcoder_tools_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn atcoder_tools_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_atcoder_tools_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_atcoder_tools_autoload_batch(&cases);
}

#[test]
fn atcoder_tools_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        commands::commands_public_surface_batch_cases(),
        configuration::configuration_public_surface_batch_cases(),
        metadata::metadata_public_surface_batch_cases(),
        registry::registry_atcoder_tools_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_atcoder_tools_batch(&cases);
}

// END generated package batch tests
