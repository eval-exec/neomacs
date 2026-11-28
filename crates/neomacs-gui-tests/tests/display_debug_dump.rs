//! Issue #855: live Wayland `neomacs-dump-display-state` bundle contract.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

/// Run the fixture once on Wayland with RUST_LOG set to RUST_LOG_VALUE and
/// return the parsed dump, its raw text and the editor's stderr.
fn run_dump(tag: &str, rust_log_value: &str) -> (Value, String, String) {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/display-debug-dump-{tag}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let dump_path = artifacts.join("live-wayland-dump.json");

    let session = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts)
        .unwrap();

    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            "display-debug-dump",
            root.join("crates/neomacs-gui-tests/fixtures/display-debug-dump.el"),
        ),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_DISPLAY_DUMP", dump_path.to_string_lossy())
    .with_env("TMPDIR", root.join("tmp").to_string_lossy())
    .with_env("RUST_LOG", rust_log_value);
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }

    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(!result.timed_out, "{result:#?}\n{stderr}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}\n{stderr}");
    assert!(stderr.contains("DISPLAY-DEBUG-PASS"), "{stderr}");

    let raw = fs::read_to_string(&dump_path)
        .unwrap_or_else(|e| panic!("failed reading {}: {e}\n{stderr}", dump_path.display()));
    let doc: Value = serde_json::from_str(&raw).expect("dump must be valid JSON");
    (doc, raw, stderr)
}

#[test]
fn wayland_display_debug_dump_captures_last_presented_and_frame_snapshot() {
    let (doc, raw, _stderr) = run_dump("prearmed", "warn,neomacs::cursor_geometry=debug");

    assert_eq!(doc["schema_version"].as_u64(), Some(1));
    assert!(
        doc["render_capture_error"].is_null(),
        "live GUI dump should capture render thread cleanly: {raw}"
    );
    assert!(
        doc["frame_snapshot_error"].is_null(),
        "live GUI dump should capture frame_snapshot cleanly: {raw}"
    );
    assert_eq!(doc["version"]["window_system"].as_str(), Some("neo"));
    assert_eq!(doc["cursor_geometry_capture_enabled"].as_bool(), Some(true));
    assert!(doc["cursor_geometry_log"].is_array());

    let frames = doc["last_presented"]
        .as_array()
        .expect("last_presented array");
    assert!(!frames.is_empty(), "expected at least one presented frame");
    let primary = frames
        .iter()
        .find(|f| f["is_primary"].as_bool() == Some(true))
        .expect("primary presented frame");
    assert!(
        primary["frame_seq"].as_u64().unwrap_or(0) >= 1,
        "frame must have been presented to swapchain: {primary}"
    );
    assert_ne!(
        primary["render_path"].as_str(),
        Some("not_yet_presented"),
        "primary frame must report a real presented render_path: {primary}"
    );
    assert!(primary["scale_factor"].as_f64().unwrap_or(0.0) > 0.0);
    assert!(primary["surface_px"][0].as_u64().unwrap_or(0) > 0);
    assert_eq!(primary["has_active_cursor"].as_bool(), Some(true));
    assert_eq!(primary["blink_enabled"].as_bool(), Some(true));
    let cursor_visible = primary["cursor_visible"]
        .as_bool()
        .expect("cursor_visible boolean");
    // Inverse video is the FILLED box's paint. Which box is drawn depends on
    // the cursor's role: the test display gives the frame no keyboard focus,
    // so GNU (`get_window_cursor_type`, xdisp.c:34670) and Neomacs draw the
    // non-selected, hollow box, which inverts nothing. The expectation follows
    // the style the draw stage reports for this cursor, not an assumed role.
    let drawn_style = doc["cursor_geometry_log"]
        .as_array()
        .expect("cursor_geometry_log array")
        .iter()
        .filter_map(Value::as_str)
        .filter(|line| line.contains(" stage=draw "))
        .filter_map(|line| line.split(" style=").nth(1)?.split(' ').next())
        .last()
        .unwrap_or_else(|| panic!("a stage=draw line with a cursor style: {raw}"))
        .to_owned();
    // Hollow is forced, not a choice: display-debug-dump.el never focuses
    // the frame and the harness grants no focus, so the frame is not the
    // highlight frame (GNU xdisp.c:34670) and `cursor-in-non-selected-windows'
    // t makes its selected window's box hollow (xdisp.c:34702-34712). A
    // regression that painted it filled must fail here.
    assert_eq!(
        drawn_style, "Hollow",
        "the unfocused frame's selected window draws the non-selected box"
    );
    assert_eq!(
        primary["inverse_video_active"].as_bool(),
        Some(cursor_visible && drawn_style == "FilledBox"),
        "inverse_video_active must be a visible FILLED box's (drawn style {drawn_style}): {primary}"
    );
    assert_eq!(
        doc["diagnostics_flags"]["cursor_hidden_or_blinked_off"].as_bool(),
        Some(!cursor_visible),
        "cursor_hidden_or_blinked_off must reflect cursor_visible: {doc}"
    );

    let snapshot_text = doc["frame_snapshot"]["text"]
        .as_array()
        .expect("frame_snapshot.text array");
    assert!(
        snapshot_text.iter().any(|t| t
            .as_str()
            .is_some_and(|s| s.contains("line 2 gamma cursor-here"))),
        "frame_snapshot.text must contain the visible buffer text: {snapshot_text:?}"
    );
    assert!(
        doc["frame_snapshot"]["geometry"]["frames"]
            .as_array()
            .is_some_and(|f| !f.is_empty())
    );
    assert_eq!(
        doc["diagnostics_flags"]["presentation_lagging"].as_bool(),
        Some(false)
    );
}

/// #855: one dump, with no environment variable arming the target, must
/// carry a fresh `stage=layout` / `stage=draw` pair. The reporter's first
/// bundle had an empty `cursor_geometry_log` because capture was armed only
/// at dump time and nothing redisplayed afterwards.
#[test]
fn wayland_display_debug_dump_self_arms_cursor_geometry_without_rust_log() {
    let (doc, raw, _stderr) = run_dump("unarmed", "warn");
    assert!(
        doc["cursor_geometry_capture_error"].is_null(),
        "the dump must capture fresh geometry on its own: {raw}"
    );
    let log: Vec<&str> = doc["cursor_geometry_log"]
        .as_array()
        .expect("cursor_geometry_log array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        log.iter().any(|line| line.contains(" stage=layout ")),
        "a stage=layout line: {log:#?}"
    );
    assert!(
        log.iter().any(|line| line.contains(" stage=draw ")),
        "a stage=draw line: {log:#?}"
    );
    assert!(
        doc["post_redisplay_last_presented"].is_array(),
        "the render state after the forced redisplay: {raw}"
    );
    assert!(
        doc["cursor_geometry_capture_wait_ms"].as_u64().is_some(),
        "{raw}"
    );
}

/// #855: the bundle's window list must carry each window's real display
/// range. The reporter's bundle had `window_end` 0 with `window_start` 3678:
/// a window's body walk recorded its range before the window's info existed,
/// so the range was dropped.
#[test]
fn wayland_display_debug_dump_window_infos_carry_the_display_range() {
    let (doc, raw, stderr) = run_dump("range", "warn");
    let frames = doc["frame_snapshot"]["geometry"]["frames"]
        .as_array()
        .expect("snapshot frames");
    let body = frames
        .iter()
        .flat_map(|frame| frame["window_infos"].as_array().into_iter().flatten())
        .find(|info| info["selected"].as_bool() == Some(true))
        .unwrap_or_else(|| panic!("a selected window info: {raw}"));
    let start = body["window_start"].as_i64().expect("window_start");
    let end = body["window_end"].as_i64().expect("window_end");
    let size = body["buffer_size"].as_i64().expect("buffer_size");
    assert!(
        start >= 1 && start < end && end <= size + 1,
        "1 <= window_start < window_end <= buffer_size + 1: start {start} end {end} size {size}"
    );
    // The presented range is what Lisp reports: (window-start) and
    // (window-end nil t), both GNU-observable.
    let lisp: Vec<i64> = stderr
        .lines()
        .find_map(|line| line.strip_prefix("LISP-WINDOW-RANGE "))
        .unwrap_or_else(|| panic!("the fixture's Lisp range: {stderr}"))
        .split_whitespace()
        .map(|field| field.parse().expect("a Lisp position"))
        .collect();
    assert_eq!(
        (start, end),
        (lisp[0], lisp[1]),
        "presented range vs Lisp (window-start) / (window-end nil t)"
    );
}
