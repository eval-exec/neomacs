use std::time::Duration;

use crate::{APEL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APEL_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APEL_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-apel-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-apel-test-file-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (string-to-list (buffer-string))))

(defun neomacs-apel-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when
          (and file (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"####;

fn apel_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(APEL_MELPA_PIN, source_file)
        .expect("prepare pinned APEL source below ./tmp")
        .with_prelude(APEL_TEST_PRELUDE)
        .with_timeout(APEL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed APEL parity test").into()
}

/// Multi-probe batch loading one source file (2a).
pub(crate) fn assert_apel_source_batch(source_file: &str, cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(apel_oracle(source_file), &name, "apel_source_batch", cases);
}

// BEGIN generated package batch tests

#[test]
fn apel_source_15e30782_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_calist_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("calist.el", &cases);
}

#[test]
fn apel_source_1f9c7678_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_mcharset_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("mcharset.el", &cases);
}

#[test]
fn apel_source_5c7e7bb5_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_path_util_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("path-util.el", &cases);
}

#[test]
fn apel_source_f9ef646e_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_pccl_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("pccl.el", &cases);
}

#[test]
fn apel_source_570df411_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_product_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("product.el", &cases);
}

#[test]
fn apel_source_3e9d8c52_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_pym_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("pym.el", &cases);
}

#[test]
fn apel_source_effc4f1f_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_richtext_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apel_source_batch("richtext.el", &cases);
}

// END generated package batch tests
