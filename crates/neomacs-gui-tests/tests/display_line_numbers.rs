//! Issue #574: real GPU gutters must follow buffer lines through replacements.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiRunStatus,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

fn painted_rows(pixels: &image::RgbaImage, colour: [u8; 3]) -> Vec<u32> {
    (0..pixels.height())
        .filter(|&y| {
            (0..pixels.width())
                .filter(|&x| pixels.get_pixel(x, y).0[..3] == colour)
                .count()
                >= 4
        })
        .collect()
}

fn check_current_line(case: &str) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/issue-574/gui-{case}-{}", std::process::id()));
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            case,
            root.join("crates/neomacs-gui-tests/fixtures/display-line-numbers.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_LINE_NUMBER_CASE", case)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let point_line = painted_rows(&pixels, [0, 0, 255]);
    let current_gutter = painted_rows(&pixels, [255, 255, 0]);
    assert!(
        !point_line.is_empty(),
        "the point line must be painted blue"
    );
    assert_eq!(
        current_gutter,
        point_line,
        "the yellow current-line gutter must align with the blue point line; artifacts: {}",
        artifacts.display()
    );
}

#[test]
fn display_string_newlines_keep_the_current_gutter_on_the_point_line() {
    check_current_line("expanded");
}

#[test]
fn hidden_buffer_newlines_keep_the_current_gutter_on_the_point_line() {
    check_current_line("hidden");
}

/// Compare live X11 getter results, including the glyph-matrix capacity
/// boundary around start line63. This intentionally requires an attested
/// oracle and a real GUI; unsupported setup is a failure, never a skip.
#[test]
fn native_line_number_gui_matrix_capacity_matches_attested_gnu() {
    use std::{fs, process::Command};

    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let forced = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the pinned GNU GUI executable"),
    );
    assert!(forced.is_absolute() && forced.is_file(), "{forced:?}");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "the GNU comparison must be attested");
    assert_eq!(fs::canonicalize(&forced).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let stamp = neomacs_test_env::fresh_build_stamp::verify(&binary, &root.join("lisp"))
        .expect("Neo GUI needs its matching complete fresh-built Lisp tree");
    assert!(matches!(
        stamp,
        neomacs_test_env::fresh_build_stamp::StampVerdict::Verified { .. }
    ));
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/line-number-gui-capacity-{}-{}",
        std::process::id(),
        neomacs_gui_tests::unique_nonce()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let prefix = forced.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&forced)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .expect("execute the prescribed GNU build/profile checker");
    fs::write(artifacts.join("gnu-attestation.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("gnu-attestation.stderr"), &check.stderr).unwrap();
    fs::write(artifacts.join("gnu-reference-stamp.txt"), oracle.stamp()).unwrap();
    assert!(check.status.success(), "GNU checker failed; {artifacts:?}");
    assert!(
        String::from_utf8_lossy(&check.stdout).contains("oracle profile: linux-gtk3"),
        "expected the pinned Linux GTK3 oracle profile; {artifacts:?}"
    );
    eprintln!(
        "{}\n{}",
        oracle.stamp(),
        String::from_utf8_lossy(&check.stdout)
    );
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let mut matrix = Vec::new();
    for geometry in ["80x40", "80x10"] {
        let gnu = run_gui_capacity_editor(
            oracle.executable(),
            "gnu",
            geometry,
            &root,
            &artifacts,
            session.env(),
        );
        let neo =
            run_gui_capacity_editor(&binary, "neo", geometry, &root, &artifacts, session.env());
        matrix.push(serde_json::json!({"geometry": geometry, "gnu": gnu, "neo": neo}));
    }
    fs::write(
        artifacts.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    // Save every observation before any mismatch aborts the comparison.
    for cell in &matrix {
        let geometry = cell["geometry"].as_str().unwrap();
        let starts: Vec<(usize, usize)> = if geometry == "80x40" {
            (58..=68).map(|line| (1100, line)).chain([(9, 1)]).collect()
        } else {
            vec![(1100, 1), (9, 1)]
        };
        for editor in ["gnu", "neo"] {
            let result = &cell[editor]["result"];
            assert_eq!(
                result["passed"], true,
                "{editor} setup: {result}; {artifacts:?}"
            );
            assert_eq!(result["geometry"], geometry);
            let rows = result["rows"]
                .as_array()
                .expect("actual complete result rows");
            assert_eq!(
                rows.len(),
                starts.len(),
                "{editor}/{geometry}; {artifacts:?}"
            );
            for (row, &(count, start)) in rows.iter().zip(&starts) {
                assert_eq!(row["case"], format!("lines-{count}-start-{start}"));
                assert_eq!(row["line_count"].as_u64(), Some(count as u64));
                assert_eq!(row["requested_start_line"].as_u64(), Some(start as u64));
                validate_gui_capacity_state(row, count, start, geometry, editor, &artifacts);
            }
        }
        let gnu_rows = cell["gnu"]["result"]["rows"].as_array().unwrap();
        let neo_rows = cell["neo"]["result"]["rows"].as_array().unwrap();
        for (gnu, neo) in gnu_rows.iter().zip(neo_rows) {
            // GNU required_matrix_height uses the window box, not outer frame chrome.
            for field in [
                "frame_width",
                "frame_height",
                "frame_char_width",
                "frame_char_height",
                "window_body_height",
                "window_body_pixels",
                "window_total_height",
                "window_total_pixels",
            ] {
                assert_eq!(
                    gnu["before"][field], neo["before"][field],
                    "SETUP: actual input geometry {field} differs for identical {geometry} argv; {artifacts:?}"
                );
            }
            let width = gnu["width_raw"].as_str().unwrap();
            let columns = gnu["columns_raw"].as_str().unwrap();
            assert!(!width.is_empty() && width.bytes().all(|c| c.is_ascii_digit()));
            assert!(width.parse::<u32>().unwrap() > 0);
            assert!(columns.contains('.') || columns.contains('e') || columns.contains('E'));
            assert!(columns.parse::<f64>().unwrap() > 0.0);
            assert_eq!(
                (&neo["width_raw"], &neo["columns_raw"]),
                (&gnu["width_raw"], &gnu["columns_raw"]),
                "GUI capacity mismatch {geometry}/{}; raw strings are untrimmed; metadata: {artifacts:?}",
                gnu["case"]
            );
        }
    }
}

fn run_gui_capacity_editor(
    binary: &std::path::Path,
    editor: &str,
    geometry: &str,
    root: &std::path::Path,
    artifact_root: &std::path::Path,
    display_env: &[(String, String)],
) -> serde_json::Value {
    use std::fs;
    let directory = artifact_root.join(geometry).join(editor);
    fs::create_dir_all(&directory).unwrap();
    let script = root.join("crates/neomacs-gui-tests/fixtures/line-number-gui-capacity.el");
    let result_path = directory.join("raw-getters-and-state.json");
    let metadata_path = directory.join("metadata-after-all-rows.json");
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        &directory,
        GuiScenario::new("line-number-gui-capacity", &script),
    )
    .with_program(binary)
    .with_args([
        "-Q".to_owned(),
        "-geometry".to_owned(),
        geometry.to_owned(),
        "-l".to_owned(),
        script.display().to_string(),
    ])
    .with_env("NEOMACS_GUI_LNUM_RESULT", result_path.display().to_string())
    .with_env(
        "NEOMACS_GUI_LNUM_METADATA",
        metadata_path.display().to_string(),
    )
    .with_env("NEOMACS_GUI_LNUM_GEOMETRY", geometry)
    .with_env("NEOMACS_GUI_LNUM_EDITOR", editor)
    .with_env("GDK_BACKEND", "x11")
    .with_env("WINIT_UNIX_BACKEND", "x11");
    for (key, value) in display_env {
        plan = plan.with_env(key, value);
    }
    let options = GuiRunOptions::with_timeout(Duration::from_secs(15));
    if editor == "gnu" {
        // GNU has no Neo GPU readback protocol. Use the same native process
        // deadline runner and retain the exact command and result instead.
        let artifacts: GuiArtifactSet = plan.write_manifest().unwrap();
        let command = plan.command_spec();
        let output = ProcessGuiCommandRunner
            .run(&command, &artifacts, &options)
            .unwrap();
        fs::write(&artifacts.stdout, &output.stdout).unwrap();
        fs::write(&artifacts.stderr, &output.stderr).unwrap();
        fs::write(
            &artifacts.json,
            serde_json::to_vec_pretty(&serde_json::json!({
                "engine": editor, "command": {"program": command.program, "args": command.args,
                "env": command.env}, "exit_code": output.exit_code, "timed_out": output.timed_out,
                "raw_result": result_path, "metadata": metadata_path
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(!output.timed_out, "GNU GUI timed out; {directory:?}");
        assert_eq!(output.exit_code, Some(0), "GNU GUI failed; {directory:?}");
    } else {
        let result = plan
            .run_with(&mut ProcessGuiCommandRunner, options)
            .unwrap();
        assert!(!result.timed_out, "{result:#?}");
        assert_eq!(result.exit_code, Some(0), "{result:#?}");
        assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    }
    serde_json::json!({
        "result": serde_json::from_slice::<serde_json::Value>(&fs::read(result_path).unwrap()).unwrap(),
        "metadata": serde_json::from_slice::<serde_json::Value>(&fs::read(metadata_path).unwrap()).unwrap()
    })
}

fn validate_gui_capacity_state(
    row: &serde_json::Value,
    count: usize,
    start: usize,
    geometry: &str,
    editor: &str,
    artifacts: &std::path::Path,
) {
    let text: String = (1..=count).map(|line| format!("line {line}\n")).collect();
    let position = 1 + text
        .lines()
        .take(start - 1)
        .map(|line| line.len() + 1)
        .sum::<usize>();
    let state = &row["before"];
    assert_eq!(
        state, &row["after"],
        "getter changed state: {editor}/{geometry}; {artifacts:?}"
    );
    for field in [
        "window_setup_seen",
        "graphic",
        "selected_window_on_selected_frame",
        "current_absolute",
    ] {
        assert_eq!(
            state[field], true,
            "SETUP {field}: {editor}/{geometry}; {state}; {artifacts:?}"
        );
    }
    assert_eq!(state["editor_role"], editor);
    let expected_window_system = match editor {
        "gnu" => "x",
        "neo" => "neo",
        other => panic!("unsupported GUI capacity editor role: {other}"),
    };
    assert_eq!(state["window_system"], expected_window_system);
    let buffer = format!("*gui-lnum-capacity-{count}*");
    assert_eq!(state["current_buffer"], buffer);
    assert_eq!(state["window_buffer"], buffer);
    assert_eq!(state["ordinary_windows"].as_u64(), Some(1));
    assert_eq!(state["frame_width"].as_u64(), Some(80));
    for field in [
        "frame_pixel_width",
        "frame_pixel_height",
        "frame_char_width",
        "frame_char_height",
        "window_body_height",
        "window_body_pixels",
        "window_total_height",
        "window_total_pixels",
    ] {
        assert!(
            state[field].as_u64().is_some_and(|value| value > 0),
            "SETUP: missing or invalid geometry {field}: {state}; {artifacts:?}"
        );
    }
    let height = state["frame_height"]
        .as_u64()
        .expect("actual positive frame height");
    assert!(
        if geometry == "80x40" {
            (30..=45).contains(&height)
        } else {
            (5..=15).contains(&height)
        },
        "SETUP: geometry argument not realized: {state}; {artifacts:?}"
    );
    assert_eq!(state["point"].as_u64(), Some(position as u64));
    assert_eq!(state["window_start"].as_u64(), Some(position as u64));
    assert_eq!(state["point_line"].as_u64(), Some(start as u64));
    assert_eq!(state["window_start_line"].as_u64(), Some(start as u64));
    assert_eq!(state["point_min"].as_u64(), Some(1));
    assert_eq!(state["point_max"].as_u64(), Some((text.len() + 1) as u64));
    assert_eq!(state["buffer_size"].as_u64(), Some(text.len() as u64));
    assert_eq!(state["line_numbers"], "t");
    assert_eq!(state["minimum_width"], "nil");
    assert_eq!(state["widen"], "nil");
    assert_eq!(state["offset"].as_i64(), Some(0));
}
