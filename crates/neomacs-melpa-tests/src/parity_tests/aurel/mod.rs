use std::time::Duration;

use crate::{AUREL_MELPA_PIN, BUI_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod filters;
mod parsing;
mod registry;
mod urls;
mod workflows;

const AUREL_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUREL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(setq temporary-file-directory "/fixture/scratch/"
      aurel-download-directory "/fixture/downloads/"
      aurel-pacman-program "/fixture/bin/pacman"
      aurel-installed-packages-check nil
      aurel-debug-level 0)

(defun aurel-test-error-data
    (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))
"##;

fn aurel_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUREL_MELPA_PIN, "aurel.el")
        .expect("prepare pinned aurel source and dependencies below ./tmp")
        .with_melpa_dependency(BUI_MELPA_PIN)
        .expect("prepare pinned BUI dependency below ./tmp")
        .with_prelude(AUREL_TEST_PRELUDE)
        .with_timeout(AUREL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aurel parity test").into()
}

/// Multi-probe batch for `assert_aurel_autoload_parity` cases (2a).
pub(crate) fn assert_aurel_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aurel_oracle().with_installed_autoloads(),
        &name,
        "aurel_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_aurel_parity` cases (2a).
pub(crate) fn assert_aurel_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aurel_oracle(), &name, "aurel_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aurel_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_aurel_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aurel_autoload_batch(&cases);
}

#[test]
fn aurel_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        filters::filters_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        registry::registry_aurel_batch_cases(),
        urls::urls_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_aurel_batch(&cases);
}

// END generated package batch tests
