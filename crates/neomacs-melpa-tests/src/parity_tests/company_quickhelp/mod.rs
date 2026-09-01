use std::time::Duration;

use crate::{COMPANY_MELPA_PIN, COMPANY_QUICKHELP_MELPA_PIN, CachedMelpaOracle, POS_TIP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COMPANY_QUICKHELP_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const COMPANY_QUICKHELP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'company)
(require 'company-quickhelp)
"####;

fn company_quickhelp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_QUICKHELP_MELPA_PIN, "company-quickhelp.el")
        .expect("prepare exact shallow company-quickhelp source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare exact shallow Company dependency below ./tmp")
        .with_melpa_dependency(POS_TIP_MELPA_PIN)
        .expect("prepare exact shallow pos-tip dependency below ./tmp")
        .with_prelude(COMPANY_QUICKHELP_TEST_PRELUDE)
        .with_timeout(COMPANY_QUICKHELP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed company-quickhelp parity test")
        .into()
}

fn assert_company_quickhelp_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        company_quickhelp_oracle(),
        &current_test_name(),
        "company_quickhelp_parity",
        cases,
    );
}

#[test]
fn company_quickhelp_package_batch() {
    assert_company_quickhelp_batch(&workflows::workflow_batch_cases());
}
