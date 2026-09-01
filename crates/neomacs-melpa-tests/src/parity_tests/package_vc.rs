use std::time::Duration;

use crate::{EmacsRuntime, run_package_vc_lifecycle};

#[test]
fn offline_package_vc_lifecycle_survives_restarts() {
    let report =
        run_package_vc_lifecycle(&EmacsRuntime::neomacs().with_timeout(Duration::from_secs(180)))
            .expect("run offline package-vc lifecycle");

    assert_eq!(
        report.checkpoints,
        [
            "installed-v1",
            "restarted-v1",
            "upgraded-v2",
            "deleted",
            "absent-after-restart",
        ]
    );
    assert_eq!(report.phases.len(), 5);
    eprintln!("{report}");
}
