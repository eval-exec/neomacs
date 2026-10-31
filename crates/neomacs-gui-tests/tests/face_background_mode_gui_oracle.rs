#![cfg(target_os = "linux")]
//! GNU oracle coverage for a graphical frame's default-face colors.
//!
//! The same fixture runs under GNU Emacs and Neomacs on one X11 display, and
//! the two result files are compared line by line. The contract it pins,
//! from GNU 31.1:
//!
//! * **Frame creation inherits the default face.** `face-set-after-frame-default`
//!   calls `internal-merge-in-global-face` for `default`, and GNU's C
//!   implementation publishes the global default face's string foreground and
//!   background colors through `Fmodify_frame_parameters`
//!   (src/xfaces.c:4398-4412). A frame created after early-init customizes the
//!   default face therefore keeps those colors.
//!
//! * **Background mode follows the published color.** Assigning
//!   `background-color` reaches `update_face_from_frame_parameter`, which calls
//!   `frame-set-background-mode` (src/xfaces.c:3896-3906). Defface display
//!   conditions such as `(background dark)` are then re-evaluated for the new
//!   frame; the fixture records which branch of a dark/light probe face won.
//!
//! * **Explicit parameters win.** `face-set-after-frame-default` applies the
//!   frame parameters passed to `make-frame` after the face merge
//!   (lisp/faces.el:2293-2305), so an explicit `background-color` overrides the
//!   default face's color.
//!
//! This is a real GNU-parity contract: it is not to be blessed from Neomacs,
//! weakened, or skipped.

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/face-background-mode-gui-oracle.el";
const SCENARIO: &str = "face-background-mode-oracle";
const RESULT_ENV: &str = "NEOMACS_FACE_BACKGROUND_MODE_ORACLE_RESULT";

fn gnu_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GNU_GUI_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"))
}

fn neomacs_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn write_diff(path: &Path, gnu: &str, neo: &str) {
    let mut diff = String::new();
    for (gnu_line, neo_line) in gnu.lines().zip(neo.lines()) {
        if gnu_line == neo_line {
            diff.push_str(&format!(" {gnu_line}\n"));
        } else {
            diff.push_str(&format!("-{gnu_line}\n+{neo_line}\n"));
        }
    }
    let _ = std::fs::write(path, diff);
}

#[test]
// Prerequisites: requires GNU GUI Emacs and Xvfb.
fn default_face_colors_and_background_mode_match_gnu() {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");

    let scenario = GuiScenario::new(SCENARIO, root.join(FIXTURE));
    let artifacts = GuiArtifactSet::new(&artifact_root, backend, SCENARIO);
    let gnu_result_path = artifact_root.join(format!("{SCENARIO}.gnu-result.el"));
    let neo_result_path = artifact_root.join(format!("{SCENARIO}.neomacs-result.el"));
    let diff_path = artifact_root.join(format!("{SCENARIO}.diff"));
    let _ = std::fs::remove_file(&gnu_result_path);
    let _ = std::fs::remove_file(&neo_result_path);

    // GNU Emacs writes the oracle file itself; the neomacs side gets the same
    // variable through the plan below.
    let mut gnu_env = vec![
        (
            RESULT_ENV.to_string(),
            gnu_result_path.display().to_string(),
        ),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
    ];
    gnu_env.extend(session.env().iter().cloned());
    let gnu_command = CommandSpec {
        program: gnu_binary(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            root.join(FIXTURE).display().to_string(),
        ],
        env: gnu_env,
    };
    let mut runner = ProcessGuiCommandRunner;
    let gnu_output = runner
        .run(
            &gnu_command,
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("GNU face oracle should run");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU face oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the GUI face oracle"
    );
    let mut plan = GuiTestPlan::new(backend, &root, &artifact_root, scenario)
        .with_program(binary)
        .with_env(RESULT_ENV, neo_result_path.display().to_string())
        .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("neomacs face oracle should run");
    assert_eq!(
        result.exit_code,
        Some(0),
        "neomacs face oracle failed; stderr:\n{}",
        result.artifacts.stderr.display()
    );

    let gnu_result = std::fs::read_to_string(&gnu_result_path)
        .unwrap_or_else(|error| panic!("GNU oracle did not write {gnu_result_path:?}: {error}"));
    let neo_result = std::fs::read_to_string(&neo_result_path).unwrap_or_else(|error| {
        panic!("Neomacs oracle did not write {neo_result_path:?}: {error}")
    });

    write_diff(&diff_path, &gnu_result, &neo_result);

    let gnu_lines = gnu_result.lines().collect::<Vec<_>>();
    let neo_lines = neo_result.lines().collect::<Vec<_>>();
    assert_eq!(
        neo_lines.len(),
        gnu_lines.len(),
        "oracle case count differs\nGNU:\n{gnu_result}\nNeomacs:\n{neo_result}"
    );

    let mut divergences = Vec::new();
    for (gnu_line, neo_line) in gnu_lines.iter().zip(&neo_lines) {
        if neo_line != gnu_line {
            divergences.push(format!("GNU:     {gnu_line}\nNeomacs: {neo_line}"));
        }
    }
    assert!(
        divergences.is_empty(),
        "GUI face state diverged from GNU in {} case(s):\n\n{}\n\ndiff: {}",
        divergences.len(),
        divergences.join("\n\n"),
        diff_path.display()
    );
}
