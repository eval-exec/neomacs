use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandOutput, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, RunnerKind,
};

#[test]
fn all_supported_backends_have_explicit_runner_kind() {
    let cases = [
        (GuiBackend::LinuxX11, RunnerKind::Xvfb),
        (GuiBackend::LinuxWayland, RunnerKind::WestonHeadless),
        (GuiBackend::Macos, RunnerKind::CurrentDesktopSession),
        (GuiBackend::Windows, RunnerKind::CurrentDesktopSession),
    ];

    for (backend, runner) in cases {
        assert_eq!(backend.runner_kind(), runner);
    }
}

#[test]
fn artifact_paths_are_backend_and_scenario_qualified() {
    let artifacts = GuiArtifactSet::new(
        PathBuf::from("target/neomacs-gui-tests"),
        GuiBackend::LinuxWayland,
        "startup-smoke",
    );

    assert_eq!(
        artifacts.json,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.json")
    );
    assert_eq!(
        artifacts.png,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.png")
    );
    assert_eq!(
        artifacts.stderr,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.stderr.log")
    );
    assert_eq!(
        artifacts.stdout,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.stdout.log")
    );
    assert_eq!(
        artifacts.gui_state,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.gui-state.json")
    );
    assert_eq!(
        artifacts.frame_snapshot_json,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.frame-snapshot.json")
    );
    assert_eq!(
        artifacts.frame_snapshot_txt,
        PathBuf::from("target/neomacs-gui-tests/linux-wayland/startup-smoke.frame-snapshot.txt")
    );
}

#[test]
fn artifact_paths_include_font_oracle_artifacts() {
    let artifacts = GuiArtifactSet::new(
        PathBuf::from("target/neomacs-gui-tests"),
        GuiBackend::LinuxX11,
        "font-selection-noto-bold",
    );

    assert_eq!(
        artifacts.gnu_font_result,
        PathBuf::from("target/neomacs-gui-tests/linux-x11/font-selection-noto-bold.gnu-result.el")
    );
    assert_eq!(
        artifacts.neomacs_font_result,
        PathBuf::from(
            "target/neomacs-gui-tests/linux-x11/font-selection-noto-bold.neomacs-result.el"
        )
    );
    assert_eq!(
        artifacts.font_oracle_diff,
        PathBuf::from(
            "target/neomacs-gui-tests/linux-x11/font-selection-noto-bold.font-oracle.diff"
        )
    );
    assert_eq!(
        artifacts.neomacs_log,
        PathBuf::from("target/neomacs-gui-tests/linux-x11/font-selection-noto-bold.neomacs.log")
    );
}

#[test]
fn font_selection_fixture_uses_matrix_cases_with_labels_and_probe_text() {
    let fixture =
        workspace_root().join("crates/neomacs-gui-tests/fixtures/font-selection-noto-bold.el");
    let contents = std::fs::read_to_string(&fixture).expect("font selection fixture should exist");

    assert!(contents.contains("neomacs-font-selection-cases"));
    assert!(contents.contains("neomacs-font-selection-weight-candidates"));
    assert!(contents.contains("neomacs-font-selection-slant-candidates"));
    assert!(contents.contains("neomacs-font-selection-size-candidates"));
    assert!(contents.contains("neomacs-font-selection-label"));
    for weight in [
        "thin",
        "ultra-light",
        "light",
        "semi-light",
        "regular",
        "medium",
        "semi-bold",
        "bold",
        "extra-bold",
        "black",
        "ultra-heavy",
    ] {
        assert!(
            contents.contains(&format!(":weight {weight}")),
            "fixture should probe GNU semantic weight candidate {weight}"
        );
    }
    for slant in [
        "reverse-oblique",
        "reverse-italic",
        "normal",
        "italic",
        "oblique",
    ] {
        assert!(
            contents.contains(&format!(":slant {slant}")),
            "fixture should probe GNU semantic slant candidate {slant}"
        );
    }
    assert!(contents.contains("\"noto-sans-weight-%s-h150-s12\""));
    assert!(contents.contains("\"noto-sans-slant-%s-h150-s12\""));
    assert!(contents.contains("\"noto-sans-size-bold-normal-h%s-s%s\""));
    assert!(contents.contains("\"Noto Sans\""));
    assert!(contents.contains(":height 220"));
    assert!(contents.contains(":size 18"));
    assert!(contents.contains("neomacs-font-selection-text \"neomacs\""));
    assert!(contents.contains(":text neomacs-font-selection-text"));
    assert!(contents.contains(":label"));
    assert!(contents.contains("font-at"));
    assert!(contents.contains("font-info"));
    assert!(contents.contains("NEOMACS_GUI_FONT_SELECTION_RESULT"));
    assert!(
        !contents.contains(":weight semibold"),
        "fixture should skip weight aliases"
    );
    assert!(
        !contents.contains(":weight heavy"),
        "fixture should skip weight aliases"
    );
}

#[test]
fn linux_x11_plan_sets_backend_and_readback_environment() {
    let scenario = GuiScenario::new("startup-smoke", "test/neomacs/neomacs-face-test.el");
    let plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        PathBuf::from("/repo"),
        PathBuf::from("/repo/target/neomacs-gui-tests"),
        scenario,
    );
    let command = plan.command_spec();

    assert_eq!(
        command.program,
        PathBuf::from("/repo/target/release/neomacs")
    );
    assert!(command.args.contains(&"-Q".into()));
    assert!(command.args.contains(&"-l".into()));
    assert!(
        command
            .args
            .contains(&"test/neomacs/neomacs-face-test.el".into())
    );
    assert_eq!(command.env_value("WINIT_UNIX_BACKEND"), Some("x11"));
    assert_eq!(
        command.env_value("NEOMACS_DEBUG_FIRST_FRAME_READBACK"),
        Some("1")
    );
    assert_eq!(
        command.env_value("NEOMACS_DEBUG_SURFACE_READBACK"),
        Some("1")
    );
    assert_eq!(
        command.env_value("NEOMACS_DEBUG_SURFACE_READBACK_PNG"),
        Some("/repo/target/neomacs-gui-tests/linux-x11/startup-smoke.png")
    );
    assert_eq!(
        command.env_value("NEOMACS_GUI_FRAME_SNAPSHOT_JSON"),
        Some("/repo/target/neomacs-gui-tests/linux-x11/startup-smoke.frame-snapshot.json")
    );
    assert_eq!(
        command.env_value("NEOMACS_GUI_FRAME_SNAPSHOT_TXT"),
        Some("/repo/target/neomacs-gui-tests/linux-x11/startup-smoke.frame-snapshot.txt")
    );
    assert_eq!(
        command.env_value("NEOMACS_GUI_FONT_SELECTION_RESULT"),
        Some("/repo/target/neomacs-gui-tests/linux-x11/startup-smoke.neomacs-result.el")
    );
}

#[test]
fn display_harness_reports_missing_linux_display_inputs() {
    let x11 = DisplayHarness::for_backend(GuiBackend::LinuxX11);
    let wayland = DisplayHarness::for_backend(GuiBackend::LinuxWayland);

    assert_eq!(x11.required_env(), &["DISPLAY"]);
    assert_eq!(
        wayland.required_env(),
        &["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY"]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn x11_session_owns_authenticated_tcp_display_below_workspace_tmp() {
    let workspace = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let root = workspace.join("tmp/neomacs-gui-tests-xvfb-contract");
    assert!(
        !root.exists(),
        "refusing pre-existing owned test root {root:?}"
    );

    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&root)
        .expect("start authenticated TCP-only Xvfb");
    let display = session
        .env()
        .iter()
        .find_map(|(name, value)| (name == "DISPLAY").then_some(value.clone()))
        .expect("session publishes DISPLAY");
    let authority = session
        .env()
        .iter()
        .find_map(|(name, value)| (name == "XAUTHORITY").then_some(PathBuf::from(value)))
        .expect("session publishes XAUTHORITY");
    assert!(display.starts_with("127.0.0.1:"), "DISPLAY was {display}");
    assert!(authority.starts_with(&root));
    assert!(authority.is_file());
    let owned_session_root = authority
        .parent()
        .expect("Xauthority is below an owned session root")
        .to_path_buf();

    let authenticated = Command::new("xdpyinfo")
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &authority)
        .output()
        .expect("launch authenticated X11 client");
    assert!(
        authenticated.status.success(),
        "authenticated X11 client failed: {}",
        String::from_utf8_lossy(&authenticated.stderr)
    );

    let unauthorized_authority = owned_session_root.join("unauthorized-Xauthority");
    std::fs::write(&unauthorized_authority, []).expect("create empty unauthorized authority");
    let unauthorized = Command::new("xdpyinfo")
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &unauthorized_authority)
        .output()
        .expect("launch unauthorized X11 client");
    assert!(
        !unauthorized.status.success(),
        "X11 server accepted a client without its generated cookie"
    );

    drop(session);
    assert!(!owned_session_root.exists());
    std::fs::remove_dir(&root).expect("remove exact empty contract root");
}

#[test]
fn test_plan_injects_display_session_environment() {
    let scenario = GuiScenario::new("startup-smoke", "test/neomacs/neomacs-face-test.el");
    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        PathBuf::from("/repo"),
        PathBuf::from("/repo/target/neomacs-gui-tests"),
        scenario,
    )
    .with_env("XDG_RUNTIME_DIR", "/tmp/neomacs-wayland")
    .with_env("WAYLAND_DISPLAY", "neomacs-gui-tests");
    let command = plan.command_spec();

    assert_eq!(command.env_value("WINIT_UNIX_BACKEND"), Some("wayland"));
    assert_eq!(
        command.env_value("XDG_RUNTIME_DIR"),
        Some("/tmp/neomacs-wayland")
    );
    assert_eq!(
        command.env_value("WAYLAND_DISPLAY"),
        Some("neomacs-gui-tests")
    );
}

#[test]
fn test_plan_can_override_neomacs_binary_path() {
    let scenario = GuiScenario::new("startup-smoke", "test/neomacs/neomacs-face-test.el");
    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        PathBuf::from("/repo"),
        PathBuf::from("/repo/target/neomacs-gui-tests"),
        scenario,
    )
    .with_program("/repo/target/release/neomacs");

    assert_eq!(
        plan.command_spec().program,
        PathBuf::from("/repo/target/release/neomacs")
    );
}

#[test]
fn test_plan_can_drive_an_init_directory_startup_surface() {
    let scenario = GuiScenario::new("startup-lifecycle", "/repo/fixtures/init/init.el");
    let plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        PathBuf::from("/repo"),
        PathBuf::from("/repo/target/neomacs-gui-tests"),
        scenario,
    )
    .with_args(["--init-directory", "/repo/fixtures/init"]);

    assert_eq!(
        plan.command_spec().args,
        vec!["--init-directory", "/repo/fixtures/init"]
    );
}

#[test]
fn test_plan_materializes_json_manifest_artifact() {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts = GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxWayland, "startup-smoke");
    let _ = std::fs::remove_file(&artifacts.json);

    let scenario = GuiScenario::new("startup-smoke", "test/neomacs/neomacs-face-test.el");
    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        scenario,
    );

    let written = plan.write_manifest().expect("manifest should be written");
    let manifest = std::fs::read_to_string(&written.json).expect("manifest should be readable");

    assert_eq!(written.json, artifacts.json);
    assert!(!written.png.exists());
    assert!(manifest.contains(r##""status":"planned""##));
    assert!(manifest.contains(r##""backend":"linux-wayland""##));
    assert!(manifest.contains(r##""runner":"weston-headless""##));
    assert!(manifest.contains(&format!(
        r##""program":"{}""##,
        workspace_root.join("target/release/neomacs").display()
    )));
    assert!(manifest.contains(r##""expected_artifacts":"##));
    assert!(manifest.contains(&format!(r##""png":"{}""##, artifacts.png.display())));
}

#[test]
fn run_with_runner_writes_ai_readable_result_artifacts() {
    let workspace_root = workspace_root();
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts = GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxWayland, "runner-success");
    let _ = std::fs::remove_file(&artifacts.json);
    let _ = std::fs::remove_file(&artifacts.png);
    let _ = std::fs::remove_file(&artifacts.stderr);
    let _ = std::fs::remove_file(&artifacts.stdout);

    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        GuiScenario::new("runner-success", "test/neomacs/neomacs-face-test.el"),
    );
    let mut runner = FakeRunner {
        output: GuiCommandOutput {
            exit_code: Some(0),
            timed_out: false,
            stdout: "ready\n".to_string(),
            stderr: "Debug surface readback: bottom_band_avg=(1.0, 2.0, 3.0, 4.0)\n".to_string(),
        },
        create_png: true,
        gui_state: None,
        frame_snapshot: None,
    };

    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(1)),
        )
        .expect("runner result should be written");
    let manifest = std::fs::read_to_string(&artifacts.json).expect("result json should exist");
    let stderr = std::fs::read_to_string(&artifacts.stderr).expect("stderr log should exist");
    let stdout = std::fs::read_to_string(&artifacts.stdout).expect("stdout log should exist");

    assert_eq!(result.status, GuiRunStatus::Passed);
    assert_eq!(result.png_bytes, Some(7));
    assert_eq!(result.stderr_bytes, stderr.len() as u64);
    assert_eq!(result.stdout_bytes, stdout.len() as u64);
    assert!(manifest.contains(r##""status":"passed""##));
    assert!(manifest.contains(r##""png_exists":true"##));
    assert!(manifest.contains(r##""readback_diagnostics":["Debug surface readback:"##));
    assert!(stderr.contains("bottom_band_avg"));
}

#[test]
fn run_with_runner_reports_missing_png_as_failed_text_result() {
    let workspace_root = workspace_root();
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts = GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxWayland, "runner-no-png");
    let _ = std::fs::remove_file(&artifacts.json);
    let _ = std::fs::remove_file(&artifacts.png);
    let _ = std::fs::remove_file(&artifacts.stderr);

    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        GuiScenario::new("runner-no-png", "test/neomacs/neomacs-face-test.el"),
    );
    let mut runner = FakeRunner {
        output: GuiCommandOutput {
            exit_code: Some(0),
            timed_out: false,
            stdout: String::new(),
            stderr: "renderer exited without readback\n".to_string(),
        },
        create_png: false,
        gui_state: None,
        frame_snapshot: None,
    };

    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(1)),
        )
        .expect("runner result should be written");
    let manifest = std::fs::read_to_string(&artifacts.json).expect("result json should exist");

    assert_eq!(result.status, GuiRunStatus::Failed);
    assert_eq!(result.png_bytes, None);
    assert!(manifest.contains(r##""status":"failed""##));
    assert!(manifest.contains(r##""png_exists":false"##));
    assert!(manifest.contains("PNG artifact was not generated"));
}

#[test]
fn run_with_runner_treats_timeout_after_png_as_successful_capture() {
    let workspace_root = workspace_root();
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts = GuiArtifactSet::new(
        &artifact_root,
        GuiBackend::LinuxWayland,
        "runner-timeout-png",
    );
    let _ = std::fs::remove_file(&artifacts.json);
    let _ = std::fs::remove_file(&artifacts.png);
    let _ = std::fs::remove_file(&artifacts.stderr);

    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        GuiScenario::new("runner-timeout-png", "test/neomacs/neomacs-face-test.el"),
    );
    let mut runner = FakeRunner {
        output: GuiCommandOutput {
            exit_code: None,
            timed_out: true,
            stdout: String::new(),
            stderr: "First-frame surface readback: ok\n".to_string(),
        },
        create_png: true,
        gui_state: None,
        frame_snapshot: None,
    };

    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(1)),
        )
        .expect("runner result should be written");
    let manifest = std::fs::read_to_string(&artifacts.json).expect("result json should exist");

    assert_eq!(result.status, GuiRunStatus::Passed);
    assert!(result.timed_out);
    assert!(manifest.contains(r##""status":"passed""##));
    assert!(manifest.contains(r##""timed_out":true"##));
}

#[test]
fn run_with_runner_includes_fixture_visible_text_snapshot() {
    let workspace_root = workspace_root();
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts =
        GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxWayland, "runner-gui-state");
    let _ = std::fs::remove_file(&artifacts.json);
    let _ = std::fs::remove_file(&artifacts.png);
    let _ = std::fs::remove_file(&artifacts.stderr);
    let _ = std::fs::remove_file(&artifacts.gui_state);

    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        GuiScenario::new("runner-gui-state", "test/neomacs/neomacs-face-test.el"),
    );
    let mut runner = FakeRunner {
        output: GuiCommandOutput {
            exit_code: Some(0),
            timed_out: false,
            stdout: String::new(),
            stderr: "First-frame surface readback: ok\n".to_string(),
        },
        create_png: true,
        gui_state: Some(r##"{"buffer_name":"*neomacs-gui-smoke*","visible_text":"NeoMacs GUI smoke line 00\nNeoMacs GUI smoke line 01\n"}"##.to_string()),
        frame_snapshot: None,
    };

    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(1)),
        )
        .expect("runner result should be written");
    let manifest = std::fs::read_to_string(&artifacts.json).expect("result json should exist");

    assert!(result.gui_state_bytes.unwrap_or_default() > 0);
    assert!(manifest.contains(r##""gui_state":"##));
    assert!(manifest.contains(r##""buffer_name":"*neomacs-gui-smoke*""##));
    assert!(manifest.contains("NeoMacs GUI smoke line 00"));
}

#[test]
fn run_with_runner_records_frame_snapshot_artifacts() {
    let workspace_root = workspace_root();
    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let artifacts = GuiArtifactSet::new(&artifact_root, GuiBackend::LinuxWayland, "runner-snap");
    let _ = std::fs::remove_file(&artifacts.json);
    let _ = std::fs::remove_file(&artifacts.png);
    let _ = std::fs::remove_file(&artifacts.stderr);
    let _ = std::fs::remove_file(&artifacts.frame_snapshot_json);
    let _ = std::fs::remove_file(&artifacts.frame_snapshot_txt);

    let plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &workspace_root,
        &artifact_root,
        GuiScenario::new("runner-snap", "test/neomacs/neomacs-face-test.el"),
    );
    let mut runner = FakeRunner {
        output: GuiCommandOutput {
            exit_code: Some(0),
            timed_out: false,
            stdout: String::new(),
            stderr: "First-frame surface readback: ok\n".to_string(),
        },
        create_png: true,
        gui_state: None,
        frame_snapshot: Some((
            r##"{"frames":[{"frame_id":1}]}"##.to_string(),
            "=== frame 1: 80x24 cols 640x384 px ===\n".to_string(),
        )),
    };

    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(1)),
        )
        .expect("runner result should be written");
    let manifest = std::fs::read_to_string(&artifacts.json).expect("result json should exist");

    assert!(result.frame_snapshot_json_bytes.unwrap_or_default() > 0);
    assert!(result.frame_snapshot_txt_bytes.unwrap_or_default() > 0);
    assert!(manifest.contains(r##""frame_snapshot_json_exists":true"##));
    assert!(manifest.contains(r##""frame_snapshot_txt_exists":true"##));
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

struct FakeRunner {
    output: GuiCommandOutput,
    create_png: bool,
    gui_state: Option<String>,
    frame_snapshot: Option<(String, String)>,
}

impl GuiCommandRunner for FakeRunner {
    fn run(
        &mut self,
        _command: &neomacs_gui_tests::CommandSpec,
        artifacts: &GuiArtifactSet,
        _options: &GuiRunOptions,
    ) -> std::io::Result<GuiCommandOutput> {
        if self.create_png {
            std::fs::create_dir_all(artifacts.png.parent().expect("png should have parent"))?;
            std::fs::write(&artifacts.png, b"not png")?;
        }
        if let Some(gui_state) = &self.gui_state {
            std::fs::create_dir_all(
                artifacts
                    .gui_state
                    .parent()
                    .expect("gui state should have parent"),
            )?;
            std::fs::write(&artifacts.gui_state, gui_state)?;
        }
        if let Some((json, txt)) = &self.frame_snapshot {
            std::fs::create_dir_all(
                artifacts
                    .frame_snapshot_json
                    .parent()
                    .expect("frame snapshot should have parent"),
            )?;
            std::fs::write(&artifacts.frame_snapshot_json, json)?;
            std::fs::write(&artifacts.frame_snapshot_txt, txt)?;
        }
        Ok(self.output.clone())
    }
}
