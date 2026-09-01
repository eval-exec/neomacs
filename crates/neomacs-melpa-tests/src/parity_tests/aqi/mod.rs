use std::time::Duration;

use crate::{AQI_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AQI_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AQI_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun aqi-test-city-data
    (city score dominant)
  `((aqi . ,score)
    (city
     (name . ,city)
     (geo . [45.274 13.721])
     (url . "https://aqicn.example/station"))
    (dominentpol . ,dominant)
    (time
     (s . "2023-05-30 12:00:00")
     (tz . "+02:00"))
    (iaqi
     (pm25 (v . 12))
     (pm10 (v . 21))
     (no2 (v . 7))
     (co (v . 3))
     (t (v . 24))
     (h (v . 61))
     (p (v . 1014))
     (wg (v . 5)))
    (attributions
     . [((name . "World Air Quality Index"))
        ((name . "Local Sensor Network"))])))

(defun aqi-test-kill-report-buffers ()
  (dolist (buffer (buffer-list))
    (when (string-prefix-p
           "*Air Quality - "
           (buffer-name buffer))
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##;

fn aqi_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AQI_MELPA_PIN, source_file)
        .expect("prepare pinned aqi source below ./tmp")
        .with_prelude(AQI_TEST_PRELUDE)
        .with_timeout(AQI_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aqi parity test").into()
}

/// Multi-probe batch for `assert_aqi_parity` cases (2a).
pub(crate) fn assert_aqi_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aqi_oracle("aqi.el"), &name, "aqi_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aqi_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aqi_batch(&cases);
}

// END generated package batch tests
