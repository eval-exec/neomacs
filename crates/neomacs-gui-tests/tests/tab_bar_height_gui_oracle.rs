//! #862: GUI tab-bar pixel height matches GNU Emacs 31.1 across X11 and Wayland.
//!
//! GNU Emacs 31.1 contract (`src/xdisp.c` at `a360712c9d`):
//! - `tab-bar` (`lisp/faces.el:2983`) inherits `variable-pitch` (`:family "Sans Serif"`,
//!   no `:box`), while `tab-bar-tab` (`lisp/tab-bar.el:48`) inherits `tab-bar` and
//!   sets `:box (:line-width 1 :style released-button)`.
//! - `tab_bar_height` (`src/xdisp.c:14946-14989`) lays out the desired tab-bar string
//!   via `display_tab_bar_line` -> `gui_produce_glyphs`. For every glyph with a boxed
//!   face, `IT_APPLY_FACE_BOX` (`src/xdisp.c:33565-33575`) adds positive
//!   `face->box_horizontal_line_width` to both `it->ascent` and `it->descent`
//!   (`+2 * line_width` total height; negative `:line-width` paints inside without
//!   expanding row height).
//! - DPI basis:
//!   - X11 (`src/xterm.c:31486-31503`): uses `Xft.dpi`, else `pixels * 25.4 / mm`
//!     (default `Xvfb 1280x800` reports `height_mm = 203` -> `100.1 DPI`, so 10pt
//!     rounds to 14 px -> `Noto Sans:pixelsize=14` height 20 px + 2 px box = 22 px).
//!   - Wayland (`src/pgtkterm.c:1133,7233`, Neomacs `FontSizing::wayland()`): uses
//!     `96.0 DPI` logical font scale, so 10pt rounds to 13 px -> `Noto Sans:pixelsize=13`
//!     height 18 px + 2 px box = 20 px (matching GNU on `Xvfb -dpi 96`).
//!   - When both backends use the same pixel size (`explicit-14px`, 14 px Sans Serif
//!     with `:box (:line-width 1)`), X11 and Wayland must both match GNU (`22 px`)
//!     and agree identically with each other.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_infra::display::start_xvfb_with_dpi;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    step: String,
    tab_bar_lines: i64,
    tab_bar_height: i64,
    root_top: i64,
    font_px: i64,
    font_height: i64,
    char_height: i64,
}

fn parse(output: &str) -> Vec<Step> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("TAB-BAR-HEIGHT "))
        .map(|record| {
            let fields: Vec<&str> = record.split_whitespace().collect();
            let [
                step,
                lines,
                height,
                root_top,
                font_px,
                font_height,
                char_height,
            ] = fields[..]
            else {
                panic!("malformed TAB-BAR-HEIGHT record: {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not a number in {record}"))
            };
            Step {
                step: step.to_owned(),
                tab_bar_lines: number(lines),
                tab_bar_height: number(height),
                root_top: number(root_top),
                font_px: number(font_px),
                font_height: number(font_height),
                char_height: number(char_height),
            }
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
) -> (Vec<Step>, String) {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/tab-bar-height.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("tab-bar-height", &fixture),
    )
    .without_paint_proof()
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
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    for (key, value) in session_env {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(90)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    let steps = parse(&output);
    let names: Vec<&str> = steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(
        names,
        ["default", "no-box", "box-2", "box-neg-2", "explicit-14px"],
        "{context}"
    );
    (steps, output)
}

fn neo_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn assert_steps_match(label: &str, gnu: &[Step], actual: &[Step], gnu_out: &str, neo_out: &str) {
    assert_eq!(
        actual, gnu,
        "{label}: tab-bar height geometry differs from GNU 31.1\n\
         gnu:\n{gnu_out}\nneo:\n{neo_out}"
    );
}

#[test]
fn tab_bar_height_matches_gnu_on_x11() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/tab-bar-height/x11");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = neo_binary(&root);

    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu-session"))
        .unwrap();
    let (gnu, gnu_out) = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        &oracle.executable_path(),
        gnu_session.env(),
    );
    drop(gnu_session);

    let neo_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let (actual, neo_out) = observe(
        &root,
        &artifacts.join("neo"),
        GuiBackend::LinuxX11,
        &neo,
        neo_session.env(),
    );

    assert_steps_match("x11 (100.1 DPI Xvfb)", &gnu, &actual, &gnu_out, &neo_out);
}

#[test]
fn tab_bar_height_matches_gnu_on_wayland() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/tab-bar-height/wayland");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = neo_binary(&root);

    // Wayland uses 96 logical DPI (matching GNU PGTK `src/pgtkterm.c:7233`);
    // start the X11 GNU oracle at matching `-dpi 96` so both editors compute
    // the exact same point-to-pixel size (`10pt -> 13px`) before comparing
    // tab-bar height and `:box` expansion.
    let gnu_session = start_xvfb_with_dpi(&artifacts.join("gnu-96dpi-session"), 1280, 800, 96)
        .expect("start 96-DPI Xvfb for GNU oracle");
    let (gnu, gnu_out) = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        &oracle.executable_path(),
        gnu_session.env(),
    );
    drop(gnu_session);

    let neo_session = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let (actual, neo_out) = observe(
        &root,
        &artifacts.join("neo"),
        GuiBackend::LinuxWayland,
        &neo,
        neo_session.env(),
    );

    assert_steps_match(
        "wayland (96 DPI Weston vs 96 DPI GNU)",
        &gnu,
        &actual,
        &gnu_out,
        &neo_out,
    );
}

#[test]
fn tab_bar_height_x11_and_wayland_agree_at_equal_font_size() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests/tab-bar-height/cross-backend");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = neo_binary(&root);

    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu-session"))
        .unwrap();
    let (gnu, gnu_out) = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        &oracle.executable_path(),
        gnu_session.env(),
    );
    drop(gnu_session);

    let x11_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("x11-session"))
        .unwrap();
    let (x11, x11_out) = observe(
        &root,
        &artifacts.join("x11"),
        GuiBackend::LinuxX11,
        &neo,
        x11_session.env(),
    );
    drop(x11_session);

    let wl_session = DisplayHarness::for_backend(GuiBackend::LinuxWayland)
        .start_session(&artifacts.join("wayland-session"))
        .unwrap();
    let (wl, wl_out) = observe(
        &root,
        &artifacts.join("wayland"),
        GuiBackend::LinuxWayland,
        &neo,
        wl_session.env(),
    );

    let explicit = |steps: &[Step]| {
        steps
            .iter()
            .find(|s| s.step == "explicit-14px")
            .cloned()
            .expect("explicit-14px step")
    };
    let gnu_14 = explicit(&gnu);
    let x11_14 = explicit(&x11);
    let wl_14 = explicit(&wl);
    assert_eq!(
        x11_14, wl_14,
        "X11 and Wayland must produce identical tab-bar height at explicit 14px font size\n\
         x11:\n{x11_out}\nwayland:\n{wl_out}"
    );
    assert_eq!(
        x11_14, gnu_14,
        "X11 and Wayland explicit 14px tab-bar height must match GNU 31.1\n\
         gnu:\n{gnu_out}\nx11:\n{x11_out}\nwayland:\n{wl_out}"
    );
}
