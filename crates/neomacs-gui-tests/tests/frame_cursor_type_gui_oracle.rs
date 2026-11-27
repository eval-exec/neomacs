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

/// The `(width, height)` of the red cursor's bounding box in PNG. A box on
/// this font is about half as wide as it is tall, a bar a few pixels wide.
fn red_bbox(png: &Path, editor: &str) -> (u32, u32) {
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
    (span(|(x, _)| *x), span(|(_, y)| *y))
}

fn shape_of((width, height): (u32, u32)) -> CursorShape {
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
    shape: Option<&str>,
) -> (u32, u32) {
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
    if let Some(shape) = shape {
        plan = plan.with_env("NEOMACS_FCT_SHAPE", shape);
    }
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
    red_bbox(&result.artifacts.png, editor)
}

/// GNU and Neomacs red-cursor bounding boxes for SHAPE (`None`: the frame
/// parameter scenario), GNU on X11 and Neomacs on NEO_BACKEND.
fn observe_both(neo_backend: GuiBackend, shape: Option<&str>) -> ((u32, u32), (u32, u32)) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("frame-cursor-type")
        .join(neo_backend.slug())
        .join(shape.map_or("frame-parameter".to_owned(), |shape| {
            shape.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        }));
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
        shape,
    );
    drop(gnu_session);
    let neo_session = DisplayHarness::for_backend(neo_backend)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let neo = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        &neo,
        &neo_session.env(),
        "neo",
        shape,
    );
    (gnu, neo)
}

fn frame_cursor_type_matches_gnu(neo_backend: GuiBackend) {
    let (gnu, neo) = observe_both(neo_backend, None);
    assert_eq!(
        shape_of(gnu),
        CursorShape::Bar,
        "precondition: GNU redraws the cursor as a bar: {gnu:?}"
    );
    assert_eq!(
        shape_of(neo),
        shape_of(gnu),
        "cursor shape after (set-frame-parameter nil 'cursor-type 'bar) differs from GNU 31.1: \
         gnu {gnu:?}, neo {neo:?}"
    );
}

/// #882 part B: an hbar and a bar wider than its glyph, on a space, sized as
/// GNU sizes them relative to the glyph: GNU paints the hbar one pixel
/// shorter than the glyph and clamps the bar to the glyph's width
/// (`x_draw_bar_cursor`, src/xterm.c). Each editor's own box cursor on the
/// same space gives its glyph size, because the cell can differ between the
/// GNU X11 reference and a Neomacs Wayland session at this font size. A bar's
/// size is compared as (width - box width, height - box height); an hbar's as
/// (width - box width, height). Frame focus does not enter: GNU narrows only a
/// bar narrower than its glyph on an unfocused frame.
fn cursor_bar_sizes_match_gnu(neo_backend: GuiBackend) {
    let (gnu_box, neo_box) = observe_both(neo_backend, Some("box"));
    let relative = |(width, height): (u32, u32), (box_width, box_height): (u32, u32), bar: bool| {
        let width = i64::from(width) - i64::from(box_width);
        let height = if bar {
            i64::from(height) - i64::from(box_height)
        } else {
            i64::from(height)
        };
        (width, height)
    };
    let mut diverging = Vec::new();
    for (shape, bar) in [("hbar", false), ("(hbar . 5)", false), ("(bar . 99)", true)] {
        let (gnu, neo) = observe_both(neo_backend, Some(shape));
        let (gnu_rel, neo_rel) = (relative(gnu, gnu_box, bar), relative(neo, neo_box, bar));
        if gnu_rel != neo_rel {
            diverging.push(format!(
                "  {shape}: gnu {gnu:?} on box {gnu_box:?} = {gnu_rel:?}; \
                 neo {neo:?} on box {neo_box:?} = {neo_rel:?}"
            ));
        }
    }
    assert!(
        diverging.is_empty(),
        "cursor size relative to its glyph differs from GNU 31.1:\n{}",
        diverging.join("\n")
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

#[test]
fn cursor_bar_sizes_match_gnu_on_x11() {
    cursor_bar_sizes_match_gnu(GuiBackend::LinuxX11);
}

#[test]
fn cursor_bar_sizes_match_gnu_on_wayland() {
    cursor_bar_sizes_match_gnu(GuiBackend::LinuxWayland);
}
