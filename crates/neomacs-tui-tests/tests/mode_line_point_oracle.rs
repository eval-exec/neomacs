#![cfg(unix)]
//! GNU authority for point and restriction effects during one mode-line walk.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = include_str!("mode_line_point_probe.el");

// GNU Emacs 31.1, interactive 40x120 xterm-256color, C.UTF-8. Collected with:
// bash tmp/editpath-cont11/run-net-cg-nix-r1.sh python3 -B
//   tmp/editpath-cont16/collect-point.py $NEOVM_FORCE_ORACLE_PATH point-gnu-r1
// The controller runs tmp/v10x/sandbox-run.sh <python> -B point-pty.py
//   $NEOVM_FORCE_ORACLE_PATH -nw -Q -no-comp-spawn
//   --eval="(setq native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent)"
//   -l crates/neomacs-tui-tests/tests/mode_line_point_probe.el
// Original GNU output is retained in tmp/editpath-cont16/point-gnu-r1/values.el.
// In particular, the earlier %l seeds GNU's base before narrowing: narrow's
// line remains 3. Recounting from the new point-min would incorrectly print 2.
const GNU_VALUES: &str = "\"start[1/0/1]forward[3/2/3][3/2/3]backward[2/4/5]narrow[3/2/3]widen[3/2/3]excursion[3/2/3]restored[1/0/1]\"\n";

#[derive(Clone, Copy, Debug)]
enum ProbeEngine {
    Gnu,
    Neomacs,
}

impl ProbeEngine {
    fn name(self) -> &'static str {
        match self {
            Self::Gnu => "GNU",
            Self::Neomacs => "Neomacs",
        }
    }
}

fn run_point_probe(engine: ProbeEngine, program: PathBuf) -> String {
    let name = engine.name();
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-point-percent-");
    let script = home.path().join("point.el");
    let out = home.path().join("point.out");
    std::fs::write(&script, PROBE_EL).expect("write point-percent probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    match engine {
        ProbeEngine::Gnu => {
            launch = launch.arg("-no-comp-spawn").arg(
                "--eval=(setq native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent)",
            );
        }
        ProbeEngine::Neomacs => {}
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("LC_ALL", "C.UTF-8")
        .env("D16_POINT_OUT", &out)
        .env("NEOMACS_MODE_LINE_GATE", "gnu");
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(60)),
        TuiProcessOutcome::Exited,
        "{name} point probe did not finish; grid:\n{}",
        session.text_grid().join("\n")
    );
    let values = std::fs::read_to_string(&out).expect("completed point-percent output");
    if let Some(record_dir) = std::env::var_os("NEOMACS_TUI_RECORD_DIR") {
        let path = PathBuf::from(record_dir).join("point-percent");
        std::fs::create_dir_all(&path).expect("create point-percent record directory");
        std::fs::write(path.join(format!("{name}.values")), &values)
            .expect("retain actual point-percent observation");
    }
    values
}

#[test]
fn point_percent_specs_follow_eval_moves_and_restrictions_like_gnu() {
    let gnu_program = std::env::var_os("ORACLE_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let gnu = run_point_probe(ProbeEngine::Gnu, gnu_program);
    assert_eq!(gnu, GNU_VALUES, "live GNU point authority changed");
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "Neomacs binary missing: {}",
        program.display()
    );
    let neo = run_point_probe(ProbeEngine::Neomacs, program);
    assert_eq!(
        neo, GNU_VALUES,
        "percent specs did not observe preceding :eval"
    );
}
