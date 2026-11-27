//! #855: the `neomacs::cursor_geometry` bug-report lines on a real frame.
//!
//! The row runs the GUI with only that target enabled, puts point on a known
//! glyph, and reads the two lines the fixture copies out of the log. It pins
//! the contract a reporter's log relies on: the draw stage names the glyph
//! under point, the drawn rectangle sits in the cursor's own row and agrees
//! with layout's rectangle, and its text-area offset is the Lisp-visible
//! `window-cursor-info` Y. The layout stage carries the scroll state.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, io, path::PathBuf, time::Duration};

/// Honor explicit compositor selection, defaulting to the host's native backend.
fn backend(configured: Option<&str>) -> GuiBackend {
    match configured {
        Some("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Some("x11" | "linux-x11") => GuiBackend::LinuxX11,
        Some("macos") => GuiBackend::Macos,
        None if cfg!(target_os = "macos") => GuiBackend::Macos,
        None => GuiBackend::LinuxX11,
        Some(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

/// The value of `key=` in LINE, up to the next space or `)`.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!(" {key}=");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find([' ', ')']).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn number(line: &str, key: &str) -> f32 {
    field(line, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no numeric {key}= in {line}"))
}

/// The `(x,y wxh)` rectangle after `key=` in LINE, as `(x, y)`.
fn rect_origin(line: &str, key: &str) -> (f32, f32) {
    let needle = format!(" {key}=(");
    let start = line
        .find(&needle)
        .unwrap_or_else(|| panic!("no {key}= rectangle in {line}"))
        + needle.len();
    let rest = &line[start..];
    let origin = &rest[..rest.find(' ').expect("rectangle origin")];
    let (x, y) = origin.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

/// The layout rectangle's `y=`, which follows `layout=(`.
fn layout_y(line: &str) -> f32 {
    let layout = &line[line.find(" layout=(").expect("layout rectangle")..];
    number(layout, "y")
}

/// `[TYPE X Y WIDTH HEIGHT ASCENT]` from the fixture's printed vector: Y.
fn cursor_info_y(info: &str) -> f32 {
    let inner = info
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("not a window-cursor-info vector: {info}"));
    inner
        .split_whitespace()
        .nth(2)
        .and_then(|y| y.parse().ok())
        .unwrap_or_else(|| panic!("no Y in {info}"))
}

#[test]
fn cursor_geometry_log_places_the_drawn_cursor_on_the_row_under_point() {
    let root = neomacs_infra::crate_root!().join("../..");
    let configured = std::env::var("NEOMACS_GUI_TEST_BACKEND").ok();
    let backend = backend(configured.as_deref());
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("cursor-geometry-telemetry");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let run_dir = artifacts
        .join(backend.slug())
        .join(format!("run-{}", std::process::id()));
    match fs::remove_dir_all(&run_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove stale {}: {error}", run_dir.display()),
    }
    fs::create_dir_all(&run_dir).unwrap();
    let result_path = run_dir.join("probe-result.txt");
    let log_path = run_dir.join("neomacs.log");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "cursor-geometry-telemetry",
            root.join("crates/neomacs-gui-tests/fixtures/cursor-geometry-telemetry.el"),
        ),
    )
    .with_program(program)
    .with_env(
        "NEOMACS_COMMIT_RESULT_FILE",
        result_path.to_string_lossy().into_owned(),
    )
    .with_env("NEOMACS_LOG_FILE", log_path.to_string_lossy().into_owned())
    .with_env("RUST_LOG", "warn,neomacs::cursor_geometry=debug");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits as soon as
            // both log lines for the cursor on `h' have been written.
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = match fs::read_to_string(&result_path) {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => panic!("read {}: {error}", result_path.display()),
    };
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");

    let marker = |prefix: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("missing {prefix}\n{context}"))
            .to_owned()
    };
    let info = marker("CURSOR-GEOMETRY-INFO ");
    let draw = marker("CURSOR-GEOMETRY-DRAW ");
    let layout = marker("CURSOR-GEOMETRY-LAYOUT ");

    // Draw stage: the glyph under point, inside its own text row.
    assert!(draw.contains(" glyph=char('h' "), "{draw}");
    assert!(draw.contains(" row=(role=Text "), "{draw}");
    assert!(draw.contains(" drawn_in_row=true "), "{draw}");
    assert!(
        draw.contains(" face=(id="),
        "realized metrics for the slot face: {draw}"
    );
    let (_, drawn_y) = rect_origin(&draw, "drawn");
    let (_, text_top) = rect_origin(&draw, "text_area");
    assert!(
        (drawn_y - layout_y(&draw)).abs() < 1.0,
        "a plain character's drawn cursor is layout's rectangle: {draw}"
    );
    // `window-cursor-info' Y is relative to the text area's top.
    let (_, info) = info.split_once(' ').expect("point and cursor info");
    assert!(
        (drawn_y - text_top - cursor_info_y(info)).abs() < 1.0,
        "drawn y - text top = window-cursor-info Y\n{draw}\n{info}"
    );

    // Layout stage: the same presentation, with its scroll state.
    assert_eq!(field(&layout, "presentation"), field(&draw, "presentation"));
    assert_eq!(field(&layout, "window"), field(&draw, "window"));
    assert_eq!(field(&layout, "hscroll"), Some("0"), "{layout}");
    assert_eq!(field(&layout, "vscroll"), Some("0"), "{layout}");
    assert!(
        field(&layout, "class").is_some_and(|class| class != "none"),
        "{layout}"
    );
}
