use std::time::Duration;

use crate::{ATL_LONG_LINES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod columns;
mod modes;
mod registry;
mod timers;
mod toggling;
mod workflows;

const ATL_LONG_LINES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ATL_LONG_LINES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun atl-long-lines-test-hook-count
    (function hook)
  (cl-count
   function
   hook
   :test #'eq))

(defun atl-long-lines-test-timer-shape
    (timer)
  (list
   (timerp timer)
   (timer--function timer)
   (timer--args timer)
   (timer--repeat-delay timer)
   (timer--idle-delay timer)))
"##;

fn atl_long_lines_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ATL_LONG_LINES_MELPA_PIN, source_file)
        .expect("prepare pinned atl-long-lines source below ./tmp")
        .with_prelude(ATL_LONG_LINES_TEST_PRELUDE)
        .with_timeout(ATL_LONG_LINES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed atl-long-lines parity test")
        .into()
}

/// Multi-probe batch for `assert_atl_long_lines_autoload_parity` cases (2a).
pub(crate) fn assert_atl_long_lines_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        atl_long_lines_oracle("atl-long-lines-autoloads.el"),
        &name,
        "atl_long_lines_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_atl_long_lines_parity` cases (2a).
pub(crate) fn assert_atl_long_lines_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        atl_long_lines_oracle("atl-long-lines.el"),
        &name,
        "atl_long_lines_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn atl_long_lines_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_atl_long_lines_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_atl_long_lines_autoload_batch(&cases);
}

#[test]
fn atl_long_lines_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        columns::columns_public_surface_batch_cases(),
        modes::modes_public_surface_batch_cases(),
        registry::registry_atl_long_lines_batch_cases(),
        timers::timers_public_surface_batch_cases(),
        toggling::toggling_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_atl_long_lines_batch(&cases);
}

// END generated package batch tests
