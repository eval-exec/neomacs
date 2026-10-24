#![cfg(unix)]
//! Live GNU authority for native percent-input reads across :eval transitions.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = include_str!("mode_line_percent_input_probe.el");

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    values: String,
    mode_line: String,
}

fn run_probe(name: &str, program: PathBuf, gnu: bool) -> Observation {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-percent-input-");
    let script = home.path().join("percent-input.el");
    let out = home.path().join("percent-input.out");
    let ready = home.path().join("percent-input.ready");
    let release = home.path().join("percent-input.release");
    std::fs::write(&script, PROBE_EL).expect("write percent-input probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("LC_ALL", "C.UTF-8")
        .env("D15_PERCENT_OUT", &out)
        .env("D15_PERCENT_READY", &ready)
        .env("D15_PERCENT_RELEASE", &release)
        .env("NEOMACS_MODE_LINE_GATE", "gnu");
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    session.read_until(Duration::from_secs(60), |_| ready.exists());
    assert!(
        ready.exists(),
        "{name} percent-input probe did not reach READY; grid:\n{}",
        session.text_grid().join("\n")
    );
    session.read(Duration::from_millis(100));
    let values = std::fs::read_to_string(&out).expect("percent-input OUT before READY");
    let mode_line = session
        .text_grid()
        .get(38)
        .expect("40-row terminal")
        .clone();
    if let Some(record_dir) = std::env::var_os("NEOMACS_TUI_RECORD_DIR") {
        let path = PathBuf::from(record_dir).join("percent-input");
        std::fs::create_dir_all(&path).expect("create percent-input record directory");
        std::fs::write(path.join(format!("{name}.values")), &values)
            .expect("record live percent-input values");
        std::fs::write(path.join(format!("{name}.row")), &mode_line)
            .expect("record live percent-input mode-line row");
    }
    std::fs::write(&release, "release\n").expect("release owned editor");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{name} percent-input probe did not exit"
    );
    Observation { values, mode_line }
}

fn assert_probe_completed(observation: &Observation, name: &str) {
    // Protocol/type-shape guards, not expected formatted strings. Both named
    // observations must contain an actual string result; a batch empty result
    // or an ERROR transcript cannot stand in for a successful live probe.
    assert!(
        observation
            .values
            .starts_with("(d15-percent-transition-v1 ((names (\"before[")
            && observation.values.contains("(flags-point (\"clean["),
        "{name} did not return the tagged nonempty transition result: {:?}",
        observation.values
    );
    assert!(
        observation.mode_line.contains("D15["),
        "{name} did not display the probe's mode line: {:?}",
        observation.mode_line
    );
}

#[test]
fn percent_inputs_follow_live_eval_like_gnu() {
    let gnu_program = std::env::var_os("ORACLE_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let gnu = run_probe("GNU", gnu_program, true);
    assert_probe_completed(&gnu, "GNU");
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "Neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, false);
    assert_probe_completed(&neo, "Neomacs");
    assert_eq!(
        neo, gnu,
        "native percent-input transitions differ; all expected values and the row come from live GNU"
    );
}
