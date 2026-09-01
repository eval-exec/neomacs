use std::time::Duration;

use crate::{AUSTRALIA_HOLIDAYS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod integration;
mod national;
mod registry;
mod states;

const AUSTRALIA_HOLIDAYS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUSTRALIA_HOLIDAYS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'calendar)
(require 'holidays)

(defun australia-holidays-test-error
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

(defun australia-holidays-test-between
    (holidays start end)
  (let ((calendar-holidays holidays))
    (sort
     (holiday-in-range
      (calendar-absolute-from-gregorian start)
      (calendar-absolute-from-gregorian end))
     #'calendar-date-compare)))

(defun australia-holidays-test-year
    (holidays year)
  (australia-holidays-test-between
   holidays
   (list 1 1 year)
   (list 12 31 year)))

(defun australia-holidays-test-year-by-symbol
    (symbol year)
  (australia-holidays-test-year
   (symbol-value symbol)
   year))

(defun australia-holidays-test-on-date
    (holidays date)
  (let ((calendar-holidays holidays))
    (calendar-check-holidays date)))
"##;

fn australia_holidays_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUSTRALIA_HOLIDAYS_MELPA_PIN, source_file)
        .expect("prepare pinned australia-holidays source below ./tmp")
        .with_prelude(AUSTRALIA_HOLIDAYS_TEST_PRELUDE)
        .with_timeout(AUSTRALIA_HOLIDAYS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed australia-holidays parity test")
        .into()
}

/// Multi-probe batch for `assert_australia_holidays_autoload_parity` cases (2a).
pub(crate) fn assert_australia_holidays_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        australia_holidays_oracle("australia-holidays-autoloads.el"),
        &name,
        "australia_holidays_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_australia_holidays_parity` cases (2a).
pub(crate) fn assert_australia_holidays_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        australia_holidays_oracle("australia-holidays.el"),
        &name,
        "australia_holidays_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn australia_holidays_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_australia_holidays_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_australia_holidays_autoload_batch(&cases);
}

#[test]
fn australia_holidays_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        integration::integration_public_surface_batch_cases(),
        national::national_public_surface_batch_cases(),
        registry::registry_australia_holidays_batch_cases(),
        states::states_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_australia_holidays_batch(&cases);
}

// END generated package batch tests
