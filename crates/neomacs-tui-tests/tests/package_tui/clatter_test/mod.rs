//! Account-free real Clatter notifications from a live package process filter.
#![cfg(target_os = "linux")]

use neomacs_melpa_test_support::{
    CLATTER_PIN, EmacsRuntime, PreparedPackageSet, clatter_notification::PrivateClatterService,
};
use neomacs_tui_tests::{TuiLaunch, TuiSession};
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn clatter_process_filter_notification_completes_and_remains_visible() {
    let root = neomacs_infra::workspace_root();
    let gnu = EmacsRuntime::gnu_emacs();
    let packages = PreparedPackageSet::from_locked_melpa(&gnu, CLATTER_PIN, "clatter-notify.el")
        .expect("prepare pinned real Clatter package");
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = root.join(format!("tmp/clatter-tui-{}-{run_id}", std::process::id()));
    let neo = std::env::var_os("NEOMACS_TUI_NEOMACS_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(neomacs_melpa_test_support::neomacs_binary);
    for (label, program) in [
        ("GNU", gnu.command().get_program().to_os_string()),
        ("NEO", neo.into_os_string()),
    ] {
        let service = PrivateClatterService::start(&artifacts.join(label)).unwrap();
        let script = service.write_client_script(&packages).unwrap();
        let mut launch = TuiLaunch::new(program)
            .args(["-nw", "-Q", "-l"])
            .arg(script.into_os_string());
        for (key, value) in service
            .environment()
            .into_iter()
            .chain(packages.process_environment())
        {
            launch = launch.env(key, value);
        }
        let mut session = TuiSession::spawn_launch(launch, label);
        let finished = |grid: &[String]| {
            grid.iter().any(|line| {
                line.contains("CLATTER-NOTIFICATION-PASSED")
                    || line.contains("CLATTER-NOTIFICATION-FAILED")
            })
        };
        session.read_until(Duration::from_secs(120), finished);
        fs::write(
            service.control().join("terminal.log"),
            session.recent_output(),
        )
        .unwrap();
        assert!(
            finished(&session.text_grid()),
            "{label}: Clatter did not finish; artifacts {:?}\n{}",
            service.control(),
            String::from_utf8_lossy(session.recent_output())
        );
        assert_eq!(
            service.notification().unwrap(),
            PrivateClatterService::expected_notification(),
            "{label}: actual native Notify payload"
        );
        assert_eq!(
            service.client_result().unwrap()["passed"],
            true,
            "{label}: actual package filter result"
        );
        assert!(
            session
                .text_grid()
                .iter()
                .any(|line| line.contains("CLATTER-NOTIFICATION-PASSED")),
            "{label}: successful native notification must remain visible"
        );
    }
}
