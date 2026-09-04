use std::path::PathBuf;
use std::time::Duration;

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiRunStatus,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};

#[test]
fn real_gui_smoke_generates_surface_readback_png() {
    let Some(backend) = requested_backend() else {
        eprintln!("skipping real GUI smoke; set NEOMACS_GUI_TEST_BACKEND=wayland or x11 to run it");
        return;
    };

    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the real GUI smoke"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let scenario = GuiScenario::new(
        "real-startup-smoke",
        workspace_root.join("crates/neomacs-gui-tests/fixtures/startup-smoke.el"),
    );
    let mut plan =
        GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario).with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(12)),
        )
        .expect("GUI run should produce text artifacts");

    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "readback PNG should be non-empty"
    );

    // Display oracle: assert what redisplay actually produced, not what the
    // fixture's Lisp said it intended.
    let snapshot_txt = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt)
        .expect("frame snapshot text artifact (rebuild target/release/neomacs if stale)");
    assert!(
        snapshot_txt.contains("=== frame "),
        "snapshot frame header:\n{snapshot_txt:.500}"
    );
    assert!(
        snapshot_txt.contains("NeoMacs GUI smoke line 00"),
        "smoke buffer text visible on screen:\n{snapshot_txt:.2000}"
    );
    assert!(
        snapshot_txt.contains("*neomacs-gui-smoke*"),
        "smoke buffer name in window header:\n{snapshot_txt:.2000}"
    );
    let snapshot_json = std::fs::read_to_string(&result.artifacts.frame_snapshot_json)
        .expect("frame snapshot JSON artifact");
    let doc: serde_json::Value =
        serde_json::from_str(&snapshot_json).expect("snapshot JSON parses");
    assert!(
        !doc["frames"].as_array().expect("frames array").is_empty(),
        "at least one frame in snapshot"
    );

    // Font render-boundary gate: every realized GUI face must carry a
    // layout-resolved font identity referencing the frame's font table, so
    // the render thread never re-selects fonts for normal text (design doc
    // 2026-07-05-font-realization-render-boundary-design.md §10/§16).
    for frame in doc["frames"].as_array().expect("frames array") {
        let fonts = frame["fonts"].as_object().expect("frame fonts table");
        assert!(
            !fonts.is_empty(),
            "frame must publish a resolved font table"
        );
        for (face_id, face) in frame["faces"].as_object().expect("faces table") {
            let resolved = &face["default_resolved_font_id"];
            assert!(
                !resolved.is_null(),
                "face {face_id} ({:?}) has no resolved font id — unresolved \
                 GUI text would hit the renderer's emergency fallback",
                face["lisp_name"]
            );
            let font_id = resolved.as_u64().expect("font id number").to_string();
            assert!(
                fonts.contains_key(&font_id),
                "face {face_id} references font {font_id} missing from the frame font table"
            );
        }
    }
}

#[test]
fn oversized_xwidget_keeps_its_intrinsic_page_visible_behind_the_window_clip() {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping oversized xwidget composition regression; WPE is Linux-only");
        return;
    }
    let Some(backend) = requested_backend() else {
        eprintln!(
            "skipping oversized xwidget composition regression; set \
             NEOMACS_GUI_TEST_BACKEND=wayland or x11 to run it"
        );
        return;
    };

    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} with --features webview before running the xwidget regression"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let scenario = GuiScenario::new(
        "oversized-xwidget",
        workspace_root.join("crates/neomacs-gui-tests/fixtures/oversized-xwidget.el"),
    );
    let mut plan = GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario)
        .with_program(binary)
        // Keep readback armed until WPE publishes its first page texture.
        .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "120");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .expect("oversized xwidget GUI run should produce artifacts");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");

    let snapshot_json = std::fs::read_to_string(&result.artifacts.frame_snapshot_json)
        .expect("oversized xwidget frame snapshot JSON");
    let snapshot: serde_json::Value =
        serde_json::from_str(&snapshot_json).expect("snapshot JSON parses");
    let (window_id, glyph) = xwidget_glyph(&snapshot)
        .expect("redisplay must retain an oversized xwidget glyph instead of dropping it");
    let xwidget = &glyph["glyph_type"]["Xwidget"];
    let intrinsic_width = xwidget["content"]["width_px"]
        .as_f64()
        .expect("xwidget intrinsic width");
    let intrinsic_height = xwidget["content"]["height_px"]
        .as_f64()
        .expect("xwidget intrinsic height");
    let layout_advance = glyph["pixel_width"]
        .as_f64()
        .expect("xwidget cropped layout advance");
    let window = snapshot["frames"]
        .as_array()
        .expect("snapshot frames")
        .iter()
        .flat_map(|frame| {
            frame["window_infos"]
                .as_array()
                .expect("frame window infos")
        })
        .find(|window| window["window_id"].as_u64() == Some(window_id))
        .expect("xwidget's window info");
    let text_body = &window["geometry"]["Complete"]["regions"]["text_body"];
    let text_width = text_body["width"].as_f64().expect("text body width");
    let text_height = text_body["height"].as_f64().expect("text body height");

    assert!(
        intrinsic_width >= text_width * 2.0 - 1.0,
        "fixture must retain an intrinsic width wider than its text area: \
         intrinsic={intrinsic_width}, text={text_width}"
    );
    assert!(
        intrinsic_height >= text_height * 2.0 - 1.0,
        "fixture must retain an intrinsic height taller than its text area: \
         intrinsic={intrinsic_height}, text={text_height}"
    );
    assert!(
        (layout_advance - text_width).abs() <= 1.0,
        "GNU crops only the row advance to the remaining text width: \
         advance={layout_advance}, text={text_width}"
    );

    let readback = image::open(&result.artifacts.png)
        .expect("decode oversized xwidget surface readback")
        .to_rgba8();
    let magenta_pixels = readback
        .pixels()
        .filter(|pixel| is_webview_magenta(pixel.0))
        .count();
    assert!(
        magenta_pixels >= 1024,
        "the semantic xwidget glyph existed, but its deterministic magenta page \
         was not visibly composed; found only {magenta_pixels} matching pixels in {}",
        result.artifacts.png.display()
    );
}

fn xwidget_glyph(snapshot: &serde_json::Value) -> Option<(u64, &serde_json::Value)> {
    for frame in snapshot["frames"].as_array()? {
        for window_matrix in frame["window_matrices"].as_array()? {
            let window_id = window_matrix["window_id"].as_u64()?;
            for row in window_matrix["matrix"]["rows"].as_array()? {
                for area in row["glyphs"].as_array()? {
                    for glyph in area.as_array()? {
                        if glyph["glyph_type"]["Xwidget"].is_object() {
                            return Some((window_id, glyph));
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_webview_magenta([red, green, blue, _alpha]: [u8; 4]) -> bool {
    red >= 200 && green <= 80 && blue >= 200
}

#[test]
fn imageless_gui_startup_runs_early_init_once_on_the_live_gui_terminal() {
    let Some(backend) = requested_backend() else {
        eprintln!(
            "skipping imageless GUI startup regression; set \
             NEOMACS_GUI_TEST_BACKEND=wayland or x11 to run it"
        );
        return;
    };

    let workspace_root = workspace_root();
    let source_binary = neomacs_binary(&workspace_root);
    assert!(
        source_binary.exists(),
        "build {source_binary:?} before running the imageless GUI startup regression"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let staged_dir = artifact_root.join(format!("issue-316-imageless-bin-{}", std::process::id()));
    if staged_dir.exists() {
        std::fs::remove_dir_all(&staged_dir).expect("clear owned imageless binary staging dir");
    }
    std::fs::create_dir_all(&staged_dir).expect("create imageless binary staging dir");
    let staged_binary = staged_dir.join(
        source_binary
            .file_name()
            .expect("Neomacs binary should have a file name"),
    );
    std::fs::copy(&source_binary, &staged_binary).expect("stage Neomacs without adjacent images");

    let init_directory = workspace_root.join("crates/neomacs-gui-tests/fixtures/issue-316-init");
    let scenario = GuiScenario::new(
        "issue-316-imageless-startup",
        init_directory.join("init.el"),
    );
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let mut plan = GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario)
        .with_program(staged_binary)
        .with_args([format!("--init-directory={}", init_directory.display())]);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("imageless GUI startup should produce artifacts");

    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state = result
        .gui_state
        .as_ref()
        .expect("startup fixture should publish GUI state");
    assert_eq!(state["early_init_count"], 1);
    assert_eq!(state["initial_window_system"], "neo");

    let stdout = std::fs::read_to_string(&result.artifacts.stdout)
        .expect("imageless startup stdout artifact");
    let stderr = std::fs::read_to_string(&result.artifacts.stderr)
        .expect("imageless startup stderr artifact");
    let output = format!("{stdout}\n{stderr}");
    assert!(
        output.contains("no runtime images found; bootstrapping from lisp sources"),
        "test must exercise the source-bootstrap fallback:\n{output:.2000}"
    );
    assert!(
        !output.contains("top-level SIGNALED")
            && !output.contains("(wrong-type-argument (terminal-live-p"),
        "source bootstrap leaked a disposable terminal into GUI startup:\n{output:.4000}"
    );
}

#[test]
fn real_gui_font_selection_oracle_matches_gnu_emacs_result_structure() {
    run_font_selection_oracle(None);
}

#[test]
fn real_gui_font_selection_semi_light_tie_matches_gnu_emacs() {
    run_font_selection_oracle(Some("noto-sans-weight-semi-light-h150-s12"));
}

#[test]
fn real_gui_font_selection_italic_entity_order_matches_gnu_emacs() {
    run_font_selection_oracle(Some("noto-sans-slant-italic-h150-s12"));
}

fn run_font_selection_oracle(case_filter: Option<&str>) {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping real GUI font selection oracle; X11 comparator is Linux-only");
        return;
    }

    let backend = GuiBackend::LinuxX11;
    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the real GUI font selection oracle"
    );
    let gnu_emacs = gnu_emacs_binary();
    assert!(
        gnu_emacs.exists(),
        "GNU Emacs binary {gnu_emacs:?} should exist for font selection oracle"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let scenario_name = case_filter
        .map(|case| format!("font-selection-{case}"))
        .unwrap_or_else(|| "font-selection-noto-bold".to_string());
    let scenario = GuiScenario::new(
        scenario_name,
        workspace_root.join("crates/neomacs-gui-tests/fixtures/font-selection-noto-bold.el"),
    );
    let artifacts = neomacs_gui_tests::GuiArtifactSet::new(&artifact_root, backend, &scenario.name);
    let _ = std::fs::remove_file(&artifacts.gnu_font_result);
    let _ = std::fs::remove_file(&artifacts.neomacs_font_result);
    let _ = std::fs::remove_file(&artifacts.font_oracle_diff);
    let _ = std::fs::remove_file(&artifacts.neomacs_log);

    let mut gnu_runner = ProcessGuiCommandRunner;
    let gnu_output = gnu_runner
        .run(
            &gnu_font_oracle_command(
                &gnu_emacs,
                &scenario.script,
                &artifacts,
                session.env(),
                case_filter,
            ),
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(12)),
        )
        .expect("GNU Emacs font oracle should run");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU Emacs oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    let mut plan = GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario)
        .with_program(binary)
        .with_env(
            "NEOMACS_LOG_FILE",
            artifacts.neomacs_log.display().to_string(),
        )
        .with_env(
            "RUST_LOG",
            "info,neomacs_renderer_wgpu::glyph_atlas=debug,neomacs::font_at=debug",
        );
    if let Some(case_filter) = case_filter {
        plan = plan.with_env("NEOMACS_GUI_FONT_SELECTION_CASE", case_filter);
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(12)),
        )
        .expect("GUI run should produce text artifacts");

    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let gnu_result = std::fs::read_to_string(&artifacts.gnu_font_result)
        .expect("GNU Emacs oracle result should be readable");
    let neomacs_result = std::fs::read_to_string(&artifacts.neomacs_font_result)
        .expect("NEO Emacs oracle result should be readable");
    write_font_oracle_diff(&artifacts.font_oracle_diff, &gnu_result, &neomacs_result)
        .expect("font oracle diff artifact should be written");

    if neomacs_result != gnu_result {
        panic!(
            "font selection oracle result diverged\nGNU Emacs result: {}\nNEO Emacs result: {}\ndiff: {}",
            artifacts.gnu_font_result.display(),
            artifacts.neomacs_font_result.display(),
            artifacts.font_oracle_diff.display(),
        );
    }
}

#[test]
fn real_gui_image_size_oracle_matches_gnu_emacs() {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping real GUI image-size oracle; X11 comparator is Linux-only");
        return;
    }

    let backend = GuiBackend::LinuxX11;
    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the real GUI image-size oracle"
    );
    let gnu_emacs = gnu_emacs_binary();
    assert!(
        gnu_emacs.exists(),
        "GNU Emacs binary {gnu_emacs:?} should exist for image-size oracle"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let dir = artifact_root.join(backend.slug());
    std::fs::create_dir_all(&dir).unwrap();
    let gnu_result_path = dir.join("image-size-oracle.gnu-result.el");
    let neomacs_result_path = dir.join("image-size-oracle.neomacs-result.el");
    let diff_path = dir.join("image-size-oracle.diff");
    let _ = std::fs::remove_file(&gnu_result_path);
    let _ = std::fs::remove_file(&neomacs_result_path);
    let _ = std::fs::remove_file(&diff_path);

    let scenario = GuiScenario::new(
        "image-size-oracle",
        workspace_root.join("crates/neomacs-gui-tests/fixtures/image-size-oracle.el"),
    );

    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");

    // GNU Emacs: the same fixture under the Xvfb display, writing its prin1
    // result to a separate file. The fixture defers image-size past startup
    // so a window-system frame exists (image-size otherwise signals
    // "Window system frame should be used").
    let artifacts = neomacs_gui_tests::GuiArtifactSet::new(&artifact_root, backend, &scenario.name);
    let mut gnu_runner = ProcessGuiCommandRunner;
    let gnu_output = gnu_runner
        .run(
            &gnu_image_oracle_command(
                &gnu_emacs,
                &scenario.script,
                &gnu_result_path,
                session.env(),
            ),
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .expect("GNU Emacs image oracle should run");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU Emacs image oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    // Neomacs: drive the same fixture through the GUI plan so it gets the
    // proven window-system startup env, adding our result-file env var.
    let mut plan = GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario)
        .with_program(binary)
        .with_env(
            "NEOMACS_GUI_IMAGE_RESULT",
            neomacs_result_path.display().to_string(),
        );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .expect("GUI run should produce the image-size oracle result");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");

    let gnu_result = std::fs::read_to_string(&gnu_result_path)
        .expect("GNU Emacs image oracle result should be readable");
    let neomacs_result = std::fs::read_to_string(&neomacs_result_path)
        .expect("NEO Emacs image oracle result should be readable");
    write_font_oracle_diff(&diff_path, &gnu_result, &neomacs_result)
        .expect("image oracle diff artifact should be written");

    if neomacs_result != gnu_result {
        panic!(
            "image-size oracle result diverged\n\
             GNU Emacs result: {}\n\
             NEO Emacs result: {}\n\
             diff: {}",
            gnu_result_path.display(),
            neomacs_result_path.display(),
            diff_path.display(),
        );
    }
}

/// Build the GNU Emacs command that runs the image oracle fixture under the
/// Xvfb display, mirroring `gnu_font_oracle_command` but writing to the
/// image result file.
fn gnu_image_oracle_command(
    program: &std::path::Path,
    script: &std::path::Path,
    result_path: &std::path::Path,
    display_env: &[(String, String)],
) -> CommandSpec {
    let mut env = vec![
        (
            "NEOMACS_GUI_IMAGE_RESULT".to_string(),
            result_path.display().to_string(),
        ),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
    ];
    env.extend(display_env.iter().cloned());
    CommandSpec {
        program: program.to_path_buf(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            script.display().to_string(),
        ],
        env,
    }
}

fn requested_backend() -> Option<GuiBackend> {
    match std::env::var("NEOMACS_GUI_TEST_BACKEND").ok()?.as_str() {
        "wayland" | "linux-wayland" => Some(GuiBackend::LinuxWayland),
        "x11" | "linux-x11" => Some(GuiBackend::LinuxX11),
        "macos" => Some(GuiBackend::Macos),
        "windows" => Some(GuiBackend::Windows),
        other => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

fn neomacs_binary(workspace_root: &std::path::Path) -> PathBuf {
    if let Some(path) = std::env::var_os("NEOMACS_GUI_TEST_BINARY") {
        return PathBuf::from(path);
    }

    workspace_root.join("target/release/neomacs")
}

fn gnu_emacs_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/exec/.local/bin/emacs"))
}

fn gnu_font_oracle_command(
    program: &std::path::Path,
    script: &std::path::Path,
    artifacts: &neomacs_gui_tests::GuiArtifactSet,
    display_env: &[(String, String)],
    case_filter: Option<&str>,
) -> CommandSpec {
    let mut env = vec![
        (
            "NEOMACS_GUI_FONT_SELECTION_RESULT".to_string(),
            artifacts.gnu_font_result.display().to_string(),
        ),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
    ];
    if let Some(case_filter) = case_filter {
        env.push((
            "NEOMACS_GUI_FONT_SELECTION_CASE".to_string(),
            case_filter.to_string(),
        ));
    }
    env.extend(display_env.iter().cloned());
    CommandSpec {
        program: program.to_path_buf(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            script.display().to_string(),
        ],
        env,
    }
}

fn write_font_oracle_diff(
    path: &std::path::Path,
    gnu_result: &str,
    neomacs_result: &str,
) -> std::io::Result<()> {
    let status = if gnu_result == neomacs_result {
        "matched"
    } else {
        "diverged"
    };
    let diff = format!(
        "status: {status}\n\n--- GNU Emacs result ---\n{gnu_result}\n--- NEO Emacs result ---\n{neomacs_result}\n"
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, diff)
}

/// GNU `deactivate-mark` (emacs-31.0.90 lisp/simple.el:7056-7066) republishes
/// the region to PRIMARY only when `gui-backend-selection-owner-p` says this
/// Emacs owns it or nobody does.  With a process-local PRIMARY (macOS,
/// Windows) ownership must therefore follow our own `gui-set-selection`, as
/// GNU NS (nsselect.m:494-511) and w32 (w32-win.el:449-451) report it.
#[test]
fn real_gui_primary_selection_follows_deactivate_mark_after_prior_ownership() {
    let Some(backend) = requested_backend() else {
        eprintln!(
            "skipping PRIMARY ownership regression; set \
             NEOMACS_GUI_TEST_BACKEND=wayland, x11 or macos to run it"
        );
        return;
    };

    let workspace_root = workspace_root();
    let binary = neomacs_binary(&workspace_root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the PRIMARY ownership regression"
    );

    let artifact_root = workspace_root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let scenario = GuiScenario::new(
        "primary-selection-ownership",
        workspace_root.join("crates/neomacs-gui-tests/fixtures/primary-selection.el"),
    );
    let mut plan =
        GuiTestPlan::new(backend, &workspace_root, &artifact_root, scenario).with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }

    let mut runner = ProcessGuiCommandRunner;
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("PRIMARY ownership run should produce artifacts");

    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state = result
        .gui_state
        .as_ref()
        .expect("PRIMARY fixture should publish GUI state");
    assert_eq!(state["error"], serde_json::Value::Null, "{state:#}");
    assert_eq!(state["window_system"], "neo");
    assert_eq!(state["select_active_regions"], true);
    assert_eq!(state["owned_before"], true, "{state:#}");
    assert_eq!(state["after_deactivate"], "new", "{state:#}");
    assert_eq!(state["owner_p"], true, "{state:#}");
    assert_eq!(state["exists_p"], true, "{state:#}");
    assert_eq!(
        state["after_disown_value"],
        serde_json::Value::Null,
        "{state:#}"
    );
    assert_eq!(state["after_disown_owner_p"], false, "{state:#}");

    let stdout = std::fs::read_to_string(&result.artifacts.stdout).expect("stdout artifact");
    let stderr = std::fs::read_to_string(&result.artifacts.stderr).expect("stderr artifact");
    let output = format!("{stdout}\n{stderr}");
    assert!(
        !output.contains("PRIMARY selection is not supported"),
        "PRIMARY must be accepted on every display backend:\n{output:.4000}"
    );
}
