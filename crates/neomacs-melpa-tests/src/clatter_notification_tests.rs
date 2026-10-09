//! Real Clatter process filters deliver notifications over an isolated D-Bus.

use crate::{EmacsRuntime, PreparedPackageSet, workspace_root};
use neomacs_melpa_test_support::{
    CLATTER_PIN, clatter_notification::PrivateClatterService, output_with_timeout,
};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn run_notification_case(
    label: &str,
    runtime: EmacsRuntime,
    packages: &PreparedPackageSet,
    artifacts: &Path,
) {
    let control = artifacts.join(label);
    let service =
        PrivateClatterService::start(&control).expect("start private GNU notification service");
    let script = service.write_client_script(packages).unwrap();
    let mut runtime = service.configure_runtime(runtime);
    for (key, value) in packages.process_environment() {
        runtime = runtime.with_env(key, value);
    }
    let mut command = runtime.command();
    command.args(["--batch", "-Q", "-l"]).arg(script);
    let output = output_with_timeout(&mut command, Duration::from_secs(90))
        .unwrap_or_else(|error| panic!("{label} Clatter client: {error:?}; artifacts {control:?}"));
    fs::write(control.join("client.stdout"), &output.stdout).unwrap();
    fs::write(control.join("client.stderr"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{label}: package filter failed; artifacts {control:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        service.client_result().unwrap(),
        serde_json::json!({
            "passed": true, "received-line": true, "filter-released": true, "exit-status": 0
        }),
        "{label}: real package filter must complete and deliver its notification"
    );
    assert_eq!(
        service.notification().unwrap(),
        PrivateClatterService::expected_notification(),
        "{label}: real Notify must preserve arrays and variant hints"
    );
}

#[test]
fn clatter_process_filter_delivers_real_notification_on_private_dbus() {
    let gnu = EmacsRuntime::gnu_emacs();
    let packages = PreparedPackageSet::from_locked_melpa(&gnu, CLATTER_PIN, "clatter-notify.el")
        .expect("install pinned Clatter through the shared MELPA package cache");
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts =
        workspace_root().join(format!("tmp/clatter-dbus-{}-{run_id}", std::process::id()));
    run_notification_case("GNU", gnu, &packages, &artifacts);
    run_notification_case("NEO", EmacsRuntime::neomacs(), &packages, &artifacts);
}
