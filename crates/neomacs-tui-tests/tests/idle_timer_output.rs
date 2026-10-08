//! Real timer and subprocess wakeups preserve the command loop's idle epoch.

use neomacs_tui_tests::{TuiLaunch, TuiSession};
use std::{fs, time::Duration};

#[test]
fn idle_timer_fires_amid_ordinary_timers_and_subprocess_output() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = root.join(format!("tmp/idle-tui-{}-{run_id}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/idle-timer-output.el");
    let neo = std::env::var_os("NEOMACS_TUI_NEOMACS_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    for (name, program) in [("GNU", std::path::PathBuf::from("emacs")), ("NEO", neo)] {
        let control = artifacts.join(name);
        fs::create_dir_all(control.join("home/.emacs.d")).unwrap();
        fs::create_dir_all(control.join("tmp")).unwrap();
        let mut launch = TuiLaunch::new(program.into_os_string())
            .args(["-nw", "-Q"])
            .env("HOME", control.join("home").into_os_string())
            .env("TMPDIR", control.join("tmp").into_os_string())
            .env("RUST_LOG", "warn")
            .env(
                "NEOMACS_LOG_FILE",
                control.join("neomacs.log").into_os_string(),
            )
            .env(
                "NEOMACS_IDLE_TEST_CONTROL",
                control.clone().into_os_string(),
            );
        if name == "GNU" {
            launch = launch.arg("-no-comp-spawn");
        }
        launch = launch.arg("-l").arg(fixture.clone().into_os_string());
        let mut session = TuiSession::spawn_launch(launch, name);
        let ready = |grid: &[String]| {
            grid.iter()
                .any(|line| line.contains("IDLE-WAITING") || line.contains("IDLE-FIRED"))
        };
        session.read_until(Duration::from_secs(20), ready);
        fs::write(
            control.join("startup-terminal.log"),
            session.recent_output(),
        )
        .unwrap();
        assert!(
            ready(&session.text_grid()),
            "{name}: fixture failed to start; artifacts: {control:?}\n{}",
            String::from_utf8_lossy(session.recent_output())
        );
        let fired = |grid: &[String]| grid.iter().any(|line| line.contains("IDLE-FIRED"));
        session.read_until(Duration::from_secs(5), fired);
        fs::write(control.join("terminal.log"), session.recent_output()).unwrap();
        assert!(
            fired(&session.text_grid()),
            "{name}: idle timer starved:\n{}",
            session.text_grid().join("\n")
        );
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(control.join("result.json")).unwrap()).unwrap();
        assert!(state["ticks"].as_u64().unwrap() > 0, "{name}: {state}");
        assert!(state["output"].as_u64().unwrap() > 0, "{name}: {state}");
        assert_eq!(
            state["worker-live"], true,
            "{name}: idle must fire while subprocess output continues: {state}"
        );
        assert_eq!(state["read-timeout"], true, "{name}: {state}");
        assert_eq!(
            state["preserved"], true,
            "{name}: timed read must preserve idle epoch: {state}"
        );
    }
}
