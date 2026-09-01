use crate::{CachedMelpaOracle, XR_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const XR_TEST_PRELUDE: &str = r##"
(require 'xr)
"##;

fn xr_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(XR_MELPA_PIN, "xr.el")
        .expect("prepare pinned xr source below ./tmp")
        .with_prelude(XR_TEST_PRELUDE)
}

#[test]
fn xr_package_batch() {
    assert_oracle_batch_cases(
        xr_oracle(),
        "xr_package_batch",
        "xr_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
