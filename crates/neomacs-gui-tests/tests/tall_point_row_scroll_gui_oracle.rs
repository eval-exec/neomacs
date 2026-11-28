//! #861: Partially visible point row taller than (or equal to) the window's
//! text area, GNU Emacs 31.1 vs Neomacs on X11 and Wayland, plus paint-proof
//! cursor clipping verification.
//!
//! GNU contracts (src/xdisp.c at pinned commit a360712c9d):
//! - `cursor_row_fully_visible_p` (:19329-19336): returns true (no scroll) for
//!   `row->height >= window_box_height` when `!force_p`, minibuffer, non-zero
//!   `vscroll`, or `cursor.vpos == 0` (window-absolute, including tab/header
//!   lines), or when `make-cursor-line-fully-visible` is nil (:19310-19311).
//! - `try_cursor_movement` (:20098-20125): for `row->height > window_box_height`
//!   on cursor-only motion, sets `*scroll_step = true` and returns
//!   `CURSOR_MOVEMENT_MUST_SCROLL`; for exact `row->height == window_box_height`,
//!   calls `cursor_row_fully_visible_p(w, false, true, false)` -> no scroll.
//! - `try_scrolling` (:19380-19704) + `redisplay_window` (:21052-21245):
//!   uses prefix glyph height for non-BOL `PT` (`line_bottom_y`, :1511-1543),
//!   starts at `extra = 0` even on non-cursor-only updates when `try_window`
//!   returns `-1` (:21052, :21718), treats overshoot past `PT` as
//!   `SCROLLING_FAILED -> recenter:`, honors `partial_line_height` at ZV
//!   (:11647-11652), uses `max(scroll_step, sc)` as `scroll_max` while capping
//!   conservative advance by `sc` (:19417, :19515), and falls through to
//!   `move_it_vertically_backward` (:11265-11434, :21232).
//! - Cursor publication & clipping: `window-cursor-info` publishes whenever
//!   `MATRIX_ROW_BOTTOM_Y(glyph_row) > 0` (:35208), including mixed-height tall
//!   rows and top-vscrolled rows, while GPU painting clips strictly to the
//!   window's text body (`x_clip_to_row`, src/xterm.c:25968).
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

const CASES: usize = 34;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CursorInfoGeom {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    ascent: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TallRowRecord {
    case: String,
    start_line: i64,
    point_line: i64,
    posn_row: Option<i64>,
    cursor_info_row: Option<i64>,
    /// Exact `[x y w h ascent]` from `window-cursor-info` for the cases whose
    /// cursor-info geometry is pinned pixel-exact (`mixed-w10-l3-on-Z`, `vscr20-l1`).
    exact_cursor_info: Option<CursorInfoGeom>,
}

fn parse_records(output: &str) -> Vec<TallRowRecord> {
    let prefix = output.split("COMMIT-EDITING-PASS").next().unwrap_or(output);
    prefix
        .lines()
        .filter_map(|line| line.strip_prefix("TALL-POINT-ROW "))
        .map(|record| {
            let fields: Vec<&str> = record.split_whitespace().collect();
            let [
                case,
                start_line,
                point_line,
                _body_px,
                line_height,
                posn_y,
                ci_x,
                ci_y,
                ci_w,
                ci_h,
                ci_a,
            ] = fields[..]
            else {
                panic!("malformed record: {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not an integer in {record}"))
            };
            let opt_num = |text: &str| -> Option<i64> {
                if text == "nil" {
                    None
                } else {
                    Some(number(text))
                }
            };
            let opt_row =
                |text: &str, lh: i64| -> Option<i64> { opt_num(text).map(|v| v.div_euclid(lh)) };
            let lh = number(line_height);
            assert!(lh > 0, "positive line height in {record}");
            let exact_cursor_info = if matches!(case, "mixed-w10-l3-on-Z" | "vscr20-l1") {
                Some(CursorInfoGeom {
                    x: opt_num(ci_x).unwrap_or_else(|| panic!("expected ci-x in {record}")),
                    y: opt_num(ci_y).unwrap_or_else(|| panic!("expected ci-y in {record}")),
                    w: opt_num(ci_w).unwrap_or_else(|| panic!("expected ci-w in {record}")),
                    h: opt_num(ci_h).unwrap_or_else(|| panic!("expected ci-h in {record}")),
                    ascent: opt_num(ci_a)
                        .unwrap_or_else(|| panic!("expected ci-ascent in {record}")),
                })
            } else {
                None
            };
            TallRowRecord {
                case: case.to_owned(),
                start_line: number(start_line),
                point_line: number(point_line),
                posn_row: opt_row(posn_y, lh),
                cursor_info_row: opt_row(ci_y, lh),
                exact_cursor_info,
            }
        })
        .collect()
}

fn assert_cursor_clipped_to_text_body(png: &Path, output: &str, backend: GuiBackend) {
    let bounds_line = output
        .lines()
        .find_map(|line| line.strip_prefix("PAINT-CLIP-BOUNDS "))
        .unwrap_or_else(|| panic!("missing PAINT-CLIP-BOUNDS on {}\n{output}", backend.slug()));
    let nums: Vec<f64> = bounds_line
        .split_whitespace()
        .map(|tok| tok.parse::<f64>().expect("numeric paint-clip bound"))
        .collect();
    let [left, top, right, bottom, frame_w, frame_h] = nums[..] else {
        panic!("malformed PAINT-CLIP-BOUNDS: {bounds_line}");
    };
    let image = image::open(png)
        .unwrap_or_else(|err| panic!("failed to open {}: {err}", png.display()))
        .to_rgb8();
    // `(frame-pixel-height)` / `(window-inside-pixel-edges)` measure the client
    // text area below the frame's menu-bar / tool-bar; the WGPU surface PNG
    // includes that top chrome band (`image.height() - frame_h * scale`).
    let scale = f64::from(image.width()) / frame_w.max(1.0);
    let top_chrome_px = (f64::from(image.height()) - frame_h * scale).max(0.0);
    let clip_left = (left * scale).floor() as u32;
    let clip_top = (top * scale + top_chrome_px).floor() as u32;
    let clip_right = (right * scale).ceil() as u32;
    let clip_bottom = (bottom * scale + top_chrome_px).ceil() as u32;

    let mut inside = 0usize;
    let mut leaked_below = 0usize;
    let mut leaked_above = 0usize;
    let mut max_y = 0u32;
    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b] = pixel.0;
        if r >= 0xf0 && g <= 0x10 && b >= 0xf0 {
            if y > max_y {
                max_y = y;
            }
            if y < clip_top {
                leaked_above += 1;
            } else if y >= clip_bottom {
                leaked_below += 1;
            } else if x >= clip_left && x < clip_right {
                inside += 1;
            }
        }
    }
    assert!(
        inside > 0,
        "expected painted magenta cursor inside text body ({clip_left}..{clip_right}, {clip_top}..{clip_bottom}) in {} on {}",
        png.display(),
        backend.slug()
    );
    assert_eq!(
        leaked_above,
        0,
        "tall cursor painted {leaked_above} magenta pixels above text_body top={clip_top} in {} on {}",
        png.display(),
        backend.slug()
    );
    assert_eq!(
        leaked_below,
        0,
        "tall cursor overdrew {leaked_below} magenta pixels below text_body bottom={clip_bottom} (max_y={max_y}) in {} on {}",
        png.display(),
        backend.slug()
    );
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
    paint_proof: bool,
) -> Vec<TallRowRecord> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/tall-point-row-scroll.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("tall-point-row-scroll", &fixture),
    )
    .with_program(binary.to_path_buf())
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env(
        "NEOMACS_COMMIT_RESULT_FILE",
        result_path.to_string_lossy().into_owned(),
    )
    .with_env("GSETTINGS_BACKEND", "memory");
    if paint_proof {
        plan = plan
            .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
            .with_env("NEOMACS_TPR_PAINT_PROOF", "1");
    } else {
        plan = plan.without_paint_proof();
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
            // Hang-avoidance cap only; the fixture exits once observations and
            // (for Neomacs) the surface-readback PNG are written.
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    if paint_proof {
        assert_cursor_clipped_to_text_body(&result.artifacts.png, &output, backend);
    }
    let records = parse_records(&output);
    assert_eq!(records.len(), CASES, "expected {CASES} cases\n{context}");
    records
}

fn tall_point_row_scroll_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("tall-point-row-scroll")
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
        false,
    );
    drop(gnu_session);

    let neo_session = DisplayHarness::for_backend(neo_backend)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let actual = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        &neo,
        &neo_session.env(),
        true,
    );

    let mut diverging: Vec<String> = Vec::new();
    for (g, n) in gnu.iter().zip(&actual) {
        let core_match = g.case == n.case
            && g.start_line == n.start_line
            && g.point_line == n.point_line
            && g.posn_row == n.posn_row
            && g.cursor_info_row == n.cursor_info_row;
        let cursor_info_match = match (&g.exact_cursor_info, &n.exact_cursor_info) {
            (Some(gc), Some(nc)) if neo_backend == GuiBackend::LinuxX11 => gc == nc,
            (Some(gc), Some(nc)) => {
                // On Wayland, GNU runs on X11 while Weston uses a different
                // default DPI for scaled `:height 10.0` fonts (152px vs 164px),
                // so x and raw y match pixel-exact while scaled h/ascent are
                // cross-checked below against the same-scale `vscr20-l1` row.
                gc.x == nc.x
                    && gc.y == nc.y
                    && (g.case != "mixed-w10-l3-on-Z" || gc.w == nc.w)
                    && nc.w > 0
                    && nc.h > 0
                    && nc.ascent > 0
                    && nc.ascent < nc.h
            }
            (None, None) => true,
            _ => false,
        };
        if !core_match || !cursor_info_match {
            diverging.push(format!("  gnu {g:?}\n  neo {n:?}"));
        }
    }
    // Cross-case invariant on every backend (including Wayland): `mixed-w10-l3-on-Z`
    // (cursor on short "Z" in a `:height 10.0` row) must publish the full
    // `glyph_row->height` and `glyph_row->ascent` matching `vscr20-l1`
    // (`src/xdisp.c:35192-35193`), not the short glyph's `ascent + descent`.
    let neo_mixed = actual
        .iter()
        .find(|r| r.case == "mixed-w10-l3-on-Z")
        .and_then(|r| r.exact_cursor_info.as_ref())
        .expect("mixed-w10-l3-on-Z cursor info");
    let neo_vscr = actual
        .iter()
        .find(|r| r.case == "vscr20-l1")
        .and_then(|r| r.exact_cursor_info.as_ref())
        .expect("vscr20-l1 cursor info");
    assert_eq!(
        (neo_mixed.h, neo_mixed.ascent),
        (neo_vscr.h, neo_vscr.ascent),
        "mixed-w10-l3-on-Z must publish full row height/ascent matching vscr20-l1 on {}: mixed={neo_mixed:?} vscr={neo_vscr:?}",
        neo_backend.slug()
    );
    assert!(
        diverging.is_empty(),
        "{} of {} #861 cases differ from GNU 31.1 on {}:\n{}",
        diverging.len(),
        gnu.len(),
        neo_backend.slug(),
        diverging.join("\n")
    );
}

#[test]
fn tall_point_row_scroll_matches_gnu_on_x11() {
    tall_point_row_scroll_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn tall_point_row_scroll_matches_gnu_on_wayland() {
    tall_point_row_scroll_matches_gnu(GuiBackend::LinuxWayland);
}
