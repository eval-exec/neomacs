//! Process string and region sends preserve configured Japanese encodings.

use neomacs_tui_tests::{TuiLaunch, TuiSession};
use std::{fs, time::Duration};

#[test]
fn japanese_process_string_and_region_sends_match_gnu_bytes() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = root.join(format!(
        "tmp/process-encoding-tui-{}-{run_id}",
        std::process::id()
    ));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/process-send-encoding.el");
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
                "NEOMACS_PROCESS_ENCODING_CONTROL",
                control.clone().into_os_string(),
            );
        if name == "GNU" {
            launch = launch.arg("-no-comp-spawn");
        }
        launch = launch.arg("-l").arg(fixture.clone().into_os_string());
        let mut session = TuiSession::spawn_launch(launch, name);
        let finished = |grid: &[String]| {
            grid.iter().any(|line| {
                line.contains("PROCESS-ENCODING-PASSED") || line.contains("PROCESS-ENCODING-FAILED")
            })
        };
        session.read_until(Duration::from_secs(20), finished);
        fs::write(control.join("terminal.log"), session.recent_output()).unwrap();
        assert!(
            finished(&session.text_grid()),
            "{name}: fixture failed to finish; artifacts {control:?}\n{}",
            String::from_utf8_lossy(session.recent_output())
        );
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(control.join("result.json")).unwrap()).unwrap();
        assert_eq!(
            state["passed"], true,
            "{name}: actual process bytes: {state}"
        );
        assert_eq!(
            state["cases"].as_array().unwrap().len(),
            26,
            "{name}: {state}"
        );
        assert_eq!(
            state["metadata"],
            serde_json::json!(["binary", "binary"]),
            "last coding must retain the installed binary descriptor: {state}"
        );
        for case in state["cases"].as_array().unwrap() {
            assert_eq!(case[2], case[3], "{name}: byte mismatch {case}");
            assert_eq!(case[4], true, "{name}: byte mismatch {case}");
        }
        assert!(
            session
                .text_grid()
                .iter()
                .any(|line| line.contains("PROCESS-ENCODING-PASSED")),
            "{name}: successful byte checks must appear in the terminal"
        );
    }
}
