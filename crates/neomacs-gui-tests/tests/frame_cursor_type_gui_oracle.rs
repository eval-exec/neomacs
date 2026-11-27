//! #882: a frame `cursor-type` parameter set after startup reshapes the GUI
//! cursor, GNU vs Neomacs, by painted pixels.
//!
//! GNU contract: `gui_set_cursor_type` calls `set_frame_cursor_types`, which
//! stores the new shape and sets `f->cursor_type_changed` "to make sure the
//! cursor gets redrawn" (src/xdisp.c:34597-34621); redisplay's idle shortcuts
//! test that flag (xdisp.c:17604, 19879). With `cursor-type` t the frame
//! parameter is the shape.
//!
//! The fixture paints a red box cursor on a large font, sets the frame
//! parameter to `bar` a second later, redisplays and exits. The row reads the
//! red pixels' bounding box from each editor's last frame image and compares
//! the shape: a bar is far narrower than it is tall. The bar's exact width
//! (GNU 1 px, Neomacs 2 px) is a separate divergence and is not compared here.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorShape {
    Box,
    Bar,
}

/// The shape of the red cursor in PNG, from its bounding box: a box on this
/// font is about half as wide as it is tall, a bar a few pixels wide.
fn cursor_shape(png: &Path, editor: &str) -> CursorShape {
    let image = image::open(png)
        .unwrap_or_else(|error| panic!("{editor}: frame image {}: {error}", png.display()))
        .to_rgb8();
    let red: Vec<(u32, u32)> = image
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel.0 == [0xff, 0x00, 0x00])
        .map(|(x, y, _)| (x, y))
        .collect();
    assert!(
        !red.is_empty(),
        "{editor}: no red cursor in {}",
        png.display()
    );
    let span = |coordinate: fn(&(u32, u32)) -> u32| {
        let values = red.iter().map(coordinate);
        values.clone().max().unwrap() - values.min().unwrap() + 1
    };
    let (width, height) = (span(|(x, _)| *x), span(|(_, y)| *y));
    if width * 4 < height {
        CursorShape::Bar
    } else {
        CursorShape::Box
    }
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
    editor: &str,
) -> CursorShape {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/frame-cursor-type.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("frame-cursor-type", &fixture),
    )
    .with_program(binary.to_path_buf())
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("GSETTINGS_BACKEND", "memory");
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    for (key, value) in session_env {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits after 2 s.
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: {result:#?}\nstderr:\n{stderr}"
    );
    assert_eq!(result.status, GuiRunStatus::Passed, "{editor}: {result:#?}");
    cursor_shape(&result.artifacts.png, editor)
}

fn frame_cursor_type_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("frame-cursor-type")
        .join(neo_backend.slug());
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));

    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu-session"))
        .unwrap();
    let gnu = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        &oracle.executable_path(),
        &gnu_session.env(),
        "gnu",
    );
    drop(gnu_session);
    assert_eq!(
        gnu,
        CursorShape::Bar,
        "precondition: GNU redraws the cursor as a bar"
    );
    let neo_session = DisplayHarness::for_backend(neo_backend)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let actual = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        &neo,
        &neo_session.env(),
        "neo",
    );
    assert_eq!(
        actual, gnu,
        "cursor shape after (set-frame-parameter nil 'cursor-type 'bar) differs from GNU 31.1"
    );
}

#[test]
fn frame_cursor_type_set_after_startup_matches_gnu_on_x11() {
    frame_cursor_type_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn frame_cursor_type_set_after_startup_matches_gnu_on_wayland() {
    frame_cursor_type_matches_gnu(GuiBackend::LinuxWayland);
}
