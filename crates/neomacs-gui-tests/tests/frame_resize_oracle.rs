#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
// Prerequisites: requires GNU GUI Emacs and Xvfb.
fn gnu_width_only_resize_preserves_native_height() {
    let root = neomacs_infra::crate_root!().join("../..");
    let artifacts = root.join("target/neomacs-gui-tests/gnu-width-resize");
    let backend = GuiBackend::LinuxX11;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "width-resize-oracle",
            root.join("crates/neomacs-gui-tests/fixtures/frame-resize-oracle.el"),
        ),
    )
    .with_program(
        std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs")),
    )
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(15)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert!(
        fs::read_to_string(result.artifacts.stderr)
            .unwrap()
            .contains("RESIZE-ORACLE columns=91")
    );
}

/// Explicit width/height after a font change is already on the X window
/// before any Lisp geometry read. GNU `adjust_frame_size` inhibit 1
/// (src/frame.c:5307, :976-995) applies it immediately. A later external
/// tool-bar ConfigureNotify (inhibit 2, src/frame.c:801-805) can shrink the
/// native height by the tool bar's pixels, so `frame-height` may be 38
/// while the outer window is still the requested grid. The startup window
/// stays near 65 columns until something flushes a queued resize.
#[test]
fn explicit_size_after_font_is_on_the_x_window_before_a_geometry_query() {
    let (gnu, actual) = sample_explicit_sizes("explicit-size-after-font", &[]);
    // Frame width 120 is true of the pre-fix binary too. The external column
    // count is what separates a delivered resize from a queued one.
    assert_eq!(gnu.frame.0, 120, "gnu width {gnu:?}");
    assert_eq!(actual.frame.0, 120, "neomacs width {actual:?}");
    assert!(
        gnu.external_cols() >= 110 && gnu.external_rows() >= 36,
        "gnu window was not the requested grid before a geometry query: {gnu:?}"
    );
    assert!(
        actual.external_cols() >= 110 && actual.external_rows() >= 36,
        "neomacs window was not the requested grid before a geometry query: {actual:?}"
    );
    assert!(
        gnu.xwin.0.abs_diff(actual.xwin.0) <= 32,
        "outer width diverged before a geometry query: neomacs {actual:?} gnu {gnu:?}"
    );
}

/// N-827-toolbar-height, window-setup-hook timing. GNU's explicit size runs
/// before the tool bar is realized, so `FRAME_TOOLBAR_HEIGHT` is still 0
/// (gtkutil.c:1181-1182). Inhibit t then keeps that outer window when the
/// bar allocates. Compared numbers are `frame-width`, `frame-height`, and
/// `frame-text-height` plus each editor's own tool-bar pixels. The menu
/// cancels, because each side subtracts the menu it put in the outer
/// request. Raw text pixels differ by the tool bar (Neo's scaled image bar
/// versus the GTK widget, xfns.c `HAVE_EXT_TOOL_BAR`). Outer pixels are not
/// compared.
#[test]
fn n827_toolbar_height_matches_gnu_frame_lines_and_text_height() {
    let (gnu, actual) = sample_explicit_sizes("n827-toolbar-height", &[]);
    assert_eq!(
        (actual.frame.0, actual.frame.1),
        (gnu.frame.0, gnu.frame.1),
        "N-827-toolbar-height frame neomacs {actual:?} gnu {gnu:?}"
    );
    assert!(
        actual.tool_height > 0 && gnu.tool_height > 0,
        "{actual:?} {gnu:?}"
    );
    assert!(
        actual.menu_height > 0 && gnu.menu_height > 0,
        "menu bar pixels neomacs {} gnu {}",
        actual.menu_height,
        gnu.menu_height
    );
    assert_eq!(
        actual.text_height + actual.tool_height,
        gnu.text_height + gnu.tool_height,
        "frame-text-height plus tool bar neomacs {actual:?} gnu {gnu:?}"
    );
}

/// Explicit size after the first redisplay. The tool bar is allocated, so
/// inhibit 1 adds it (gtkutil.c:1181-1182). `frame-height` and
/// `frame-text-height` are the compared numbers, under `t`,
/// `(tool-bar-lines)`, and the default.
fn assert_explicit_size_after_redisplay(inhibit: &str) {
    let (gnu, actual) = sample_explicit_sizes(
        &format!("n827-after-redisplay-{inhibit}"),
        &[("N827_INHIBIT", inhibit), ("N827_WHEN", "redisplay")],
    );
    assert_eq!(
        (actual.frame.0, actual.frame.1, actual.text_height),
        (gnu.frame.0, gnu.frame.1, gnu.text_height),
        "inhibit {inhibit} neomacs {actual:?} gnu {gnu:?}"
    );
}

#[test]
fn n827_after_redisplay_inhibit_t_keeps_requested_text_height() {
    assert_explicit_size_after_redisplay("t");
}

#[test]
fn n827_after_redisplay_inhibit_tool_bar_lines_keeps_requested_text_height() {
    assert_explicit_size_after_redisplay("tool-bar-lines");
}

#[test]
fn n827_after_redisplay_default_inhibit_keeps_requested_text_height() {
    assert_explicit_size_after_redisplay("default");
}

/// Window-setup-hook size, sampled after the command loop redisplays.
/// Default inhibit grows on the first item-bearing redisplay, so the
/// requested 40 rows and raw `frame-text-height` match GNU.
#[test]
fn n827_window_setup_hook_default_inhibit_grows_to_keep_text_height() {
    let (gnu, actual) = sample_explicit_sizes("n827-hook-default", &[("N827_INHIBIT", "default")]);
    assert_eq!(
        (actual.frame.0, actual.frame.1, actual.text_height),
        (gnu.frame.0, gnu.frame.1, gnu.text_height),
        "hook default neomacs {actual:?} gnu {gnu:?}"
    );
    assert_eq!(actual.frame.1, 40, "neomacs {actual:?} gnu {gnu:?}");
}

/// `(tool-bar-lines)` at hook time preserves the outer window. Compared
/// numbers are frame lines plus each editor's text height and tool-bar
/// pixels, the same contract as inhibit t.
#[test]
fn n827_window_setup_hook_tool_bar_lines_keeps_lines_and_text_plus_tool() {
    let (gnu, actual) = sample_explicit_sizes(
        "n827-hook-tool-bar-lines",
        &[("N827_INHIBIT", "tool-bar-lines")],
    );
    assert_eq!(
        (actual.frame.0, actual.frame.1),
        (gnu.frame.0, gnu.frame.1),
        "hook tool-bar-lines frame neomacs {actual:?} gnu {gnu:?}"
    );
    assert!(
        actual.tool_height > 0 && gnu.tool_height > 0,
        "{actual:?} {gnu:?}"
    );
    assert_eq!(
        actual.text_height + actual.tool_height,
        gnu.text_height + gnu.tool_height,
        "hook tool-bar-lines text plus tool neomacs {actual:?} gnu {gnu:?}"
    );
}

/// Explicit size inside `window-setup-hook`, sampled before the command
/// loop redisplays. Neither editor has realized the tool bar, so
/// `frame-height` is 40 and `frame-text-height` matches. Outer pixels are
/// not the contract: the window may still be unmapped (`xwin` 0).
#[test]
fn n827_explicit_size_before_first_redisplay_keeps_requested_rows() {
    let (gnu, actual) = sample_explicit_sizes(
        "n827-before-redisplay",
        &[("N827_WHEN", "before-redisplay")],
    );
    assert_eq!(
        (actual.frame.0, actual.frame.1, actual.text_height),
        (gnu.frame.0, gnu.frame.1, gnu.text_height),
        "before redisplay neomacs {actual:?} gnu {gnu:?}"
    );
    assert_eq!(actual.frame.1, 40, "neomacs {actual:?} gnu {gnu:?}");
}

/// `(redisplay)` and `(sit-for 0)` during init allocate the bar before the
/// hook's explicit size. Inhibit t then includes the bar, so the sampled
/// frame stays 40 rows and raw text height matches.
#[test]
fn n827_init_redisplay_allocates_before_the_explicit_size() {
    let (gnu, actual) =
        sample_explicit_sizes("n827-init-redisplay", &[("N827_WHEN", "init-redisplay")]);
    assert_eq!(
        (actual.frame.0, actual.frame.1, actual.text_height),
        (gnu.frame.0, gnu.frame.1, gnu.text_height),
        "init redisplay neomacs {actual:?} gnu {gnu:?}"
    );
    assert_eq!(actual.frame.1, 40, "neomacs {actual:?} gnu {gnu:?}");
}

fn sample_explicit_sizes(
    artifact_name: &str,
    extra_env: &[(&str, &str)],
) -> (ExplicitSize, ExplicitSize) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests").join(artifact_name);
    let _ = fs::remove_dir_all(&artifacts);
    fs::create_dir_all(&artifacts).unwrap();
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let backend = GuiBackend::LinuxX11;
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let gnu = observe_explicit_size(
        &root,
        &artifacts.join("gnu"),
        oracle.executable_path(),
        &session,
        extra_env,
    );
    let actual = observe_explicit_size(&root, &artifacts.join("neo"), neo, &session, extra_env);
    (gnu, actual)
}

#[derive(Debug)]
struct ExplicitSize {
    xwin: (u32, u32),
    frame: (u32, u32),
    char_px: (u32, u32),
    text_height: u32,
    tool_height: u32,
    menu_height: u32,
}

impl ExplicitSize {
    fn external_cols(&self) -> u32 {
        self.xwin.0 / self.char_px.0.max(1)
    }

    fn external_rows(&self) -> u32 {
        self.xwin.1 / self.char_px.1.max(1)
    }
}

fn observe_explicit_size(
    root: &std::path::Path,
    artifacts: &std::path::Path,
    binary: PathBuf,
    session: &neomacs_gui_tests::DisplaySession,
    extra_env: &[(&str, &str)],
) -> ExplicitSize {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/explicit-size-after-font-oracle.el");
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        artifacts,
        GuiScenario::new("explicit-size-after-font", &fixture),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_args([
        "-Q".to_string(),
        "--font=DejaVu Sans Mono-10".to_string(),
        "--geometry=65x29".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("GDK_BACKEND", "x11")
    .with_env("GSETTINGS_BACKEND", "memory");
    for (key, value) in extra_env {
        plan = plan.with_env(*key, *value);
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap();
    parse_explicit_size(&stderr)
}

fn parse_explicit_size(stderr: &str) -> ExplicitSize {
    let line = stderr
        .lines()
        .find(|line| line.contains("EXPLICIT-SIZE"))
        .unwrap_or_else(|| panic!("missing EXPLICIT-SIZE line: {stderr}"));
    let mut xwin = (0, 0);
    let mut frame = (0, 0);
    let mut char_px = (0, 0);
    let mut text_height = None;
    let mut tool_height = None;
    let mut menu_height = None;
    for field in line.split_whitespace() {
        if let Some(value) = field.strip_prefix("xwin=") {
            xwin = parse_pair(value);
        } else if let Some(value) = field.strip_prefix("frame=") {
            frame = parse_pair(value);
        } else if let Some(value) = field.strip_prefix("char=") {
            char_px = parse_pair(value);
        } else if let Some(value) = field.strip_prefix("text=") {
            text_height = value.parse().ok();
        } else if let Some(value) = field.strip_prefix("tool=") {
            tool_height = value.parse().ok();
        } else if let Some(value) = field.strip_prefix("menu=") {
            menu_height = value.parse().ok();
        }
    }
    ExplicitSize {
        xwin,
        frame,
        char_px,
        text_height: text_height.unwrap_or_else(|| panic!("missing text height: {line}")),
        tool_height: tool_height.unwrap_or_else(|| panic!("missing tool height: {line}")),
        menu_height: menu_height.unwrap_or_else(|| panic!("missing menu height: {line}")),
    }
}

fn parse_pair(value: &str) -> (u32, u32) {
    let (width, height) = value.split_once('x').unwrap();
    (width.parse().unwrap(), height.parse().unwrap())
}

#[path = "frame_resize_oracle/native_rejection.rs"]
mod native_rejection;
