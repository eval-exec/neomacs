#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_opted_in_frame_follows_live_desktop_font() {
    check_live_font(GuiBackend::LinuxX11, "gnu-live-font", LiveFontCase::OptIn);
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts.
fn opted_in_frame_follows_live_desktop_font() {
    check_live_font(GuiBackend::LinuxWayland, "live-font", LiveFontCase::OptIn);
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_opted_out_frame_keeps_its_font_while_queries_refresh() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-opt-out",
        LiveFontCase::OptOut,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts.
fn opted_out_frame_keeps_its_font_while_queries_refresh() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-opt-out",
        LiveFontCase::OptOut,
    );
}

#[derive(strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum LiveFontCase {
    OptIn,
    OptOut,
    ExplicitAndFuture,
    Geometry,
    Repeated,
    Inhibited,
    Child,
    ChildChrome,
    ChildScrollbar,
    Minimum,
    MinimumVertical,
    GrownMinibuffer,
    /// 85x24 and 91x25 resizes plus a font change in one command-loop turn.
    OverlappingResize,
    /// The same requests, with the font change after the settled 91x25 grid.
    SettledResizeThenFont,
    Fullscreen,
}

/// The same stimulus runs on both editors: 80x24, then 85x24 and 91x25 resizes
/// plus a font change in one command-loop turn. Each editor then settles under
/// its own event processing. The fixture accepts an end state only after it
/// has held unchanged for 1 s (re-checked every 50 ms), so a transient grid is
/// never compared. That hold is a stability convergence, not a blind settle:
/// a change resets it and is logged as LIVE-FONT-SETTLE-RESET. It lives in
/// `fixtures/desktop-font-live.el`, which the wall-clock guard deliberately
/// does not scan (Elisp fixtures are outside it). Review is what keeps it from
/// becoming a fixed sleep before a single observation.
///
/// GNU 31.1 can end this sequence on more than one grid, depending on whether
/// the `ConfigureNotify` for a resize is processed before `x_new_font` runs.
/// `x_new_font` sizes the frame from the current `FRAME_COLS`/`FRAME_LINES`
/// (xterm.c:27179-27181). `xg_frame_set_char_size` waits for each configure
/// only up to the 0.1 s `x-wait-for-event-timeout` (gtkutil.c:1274,
/// xterm.c:28432). Neomacs always ends on 91x25, which is the outcome GNU
/// produces when it wins that race. A program that depends on which side of
/// the race GNU lands on is not portable under GNU itself. These rows
/// therefore assert the set of states each editor can legitimately settle in,
/// and the settled-resize rows assert the common state.
///
/// How the GNU set was obtained: 22 attested-oracle runs on a loaded host
/// (load average 42-52 on 32 cores).
/// - 12 runs with the 1 s settle hold: 6 settled on 91x25 and 6 on 85x24,
///   with no reset in any run.
/// - 10 earlier runs without the hold: 8 on 91x25 and 2 on 85x24.
///
/// 80x24 (neither configure processed in time) was never observed under load
/// alone. It is the grid `x_new_font` reads when both waits miss, and it was
/// produced directly by running the same row with `--eval '(setq
/// x-wait-for-event-timeout nil)'`, so that `x_wait_for_event` returns at
/// once. 15 such runs settled 10x on 80x24 and 5x on 85x24, never 91x25, all
/// held with no reset. That run is not deterministic: the GTK event flush in
/// `xg_frame_set_char_size` sometimes still drains the first configure. With
/// the earlier `{85x24, 91x25}` set, the same configuration timed out at the
/// 60 s cap on a settled 80x24 frame, so the narrower set was a flake waiting
/// for a slow host.
///
/// These are the states we can produce or argue for from the source. That is
/// not a proof that GNU 31.1 can reach no other grid. If this row fails on
/// a settled state outside the set, widen the set deliberately, with the same
/// evidence: an attested run that produces the state, and the source path
/// that explains it.
#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_overlapping_resize_and_font_change_settle_on_a_requested_grid() {
    check_live_font_grids(
        GuiBackend::LinuxX11,
        "gnu-live-overlapping",
        LiveFontCase::OverlappingResize,
        &[(80, 24), (85, 24), (91, 25)],
    );
}

/// Neomacs settles the same stimulus on the latest requested grid. See the
/// GNU row above for why this is one of GNU's own settled states and not a
/// divergence.
#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn overlapping_resize_and_font_change_settle_on_the_latest_grid() {
    check_live_font_grids(
        GuiBackend::LinuxWayland,
        "live-overlapping",
        LiveFontCase::OverlappingResize,
        &[(91, 25)],
    );
}

/// With the font change issued only after the 91x25 grid has settled, both
/// editors must keep 91x25 under the new font.
#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_settled_resize_then_font_change_preserves_grid() {
    check_live_font_grids(
        GuiBackend::LinuxX11,
        "gnu-live-settled-resize",
        LiveFontCase::SettledResizeThenFont,
        &[(91, 25)],
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn settled_resize_then_font_change_preserves_grid() {
    check_live_font_grids(
        GuiBackend::LinuxWayland,
        "live-settled-resize",
        LiveFontCase::SettledResizeThenFont,
        &[(91, 25)],
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_grown_minibuffer_retains_frame_height_across_resize_and_font_change() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-grown-mini",
        LiveFontCase::GrownMinibuffer,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn grown_minibuffer_retains_frame_height_across_resize_and_font_change() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-grown-mini",
        LiveFontCase::GrownMinibuffer,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston presentation feedback, GSettings and fonts.
fn fullscreen_font_update_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-fullscreen",
        LiveFontCase::Fullscreen,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_live_font_preserves_child_frame_grid() {
    check_live_font(GuiBackend::LinuxX11, "gnu-live-child", LiveFontCase::Child);
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn live_font_preserves_child_frame_grid() {
    check_live_font(GuiBackend::LinuxWayland, "live-child", LiveFontCase::Child);
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_child_chrome_preserves_initial_and_live_font_text_grid() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-child-chrome",
        LiveFontCase::ChildChrome,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn child_chrome_preserves_initial_and_live_font_text_grid() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-child-chrome",
        LiveFontCase::ChildChrome,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_child_scrollbar_and_distinct_font_preserve_initial_text_grid() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-child-scrollbar",
        LiveFontCase::ChildScrollbar,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn child_scrollbar_and_distinct_font_preserve_initial_text_grid() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-child-scrollbar",
        LiveFontCase::ChildScrollbar,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_stacked_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-minimum-vertical",
        LiveFontCase::MinimumVertical,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn stacked_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-minimum-vertical",
        LiveFontCase::MinimumVertical,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and fonts.
fn gnu_split_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-minimum",
        LiveFontCase::Minimum,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and fonts.
fn split_window_minimum_overrides_font_resize_inhibition() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-minimum",
        LiveFontCase::Minimum,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_inhibited_font_resize_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-inhibited",
        LiveFontCase::Inhibited,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston presentation feedback, GSettings and fonts.
fn inhibited_font_resize_keeps_pixels_and_refreshes_columns() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-inhibited",
        LiveFontCase::Inhibited,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_duplicate_font_updates_are_noops_and_distinct_updates_repeat() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-repeated",
        LiveFontCase::Repeated,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston presentation feedback, GSettings and fonts.
fn duplicate_font_updates_are_noops_and_distinct_updates_repeat() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-repeated",
        LiveFontCase::Repeated,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_live_font_preserves_the_character_grid() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-geometry",
        LiveFontCase::Geometry,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston presentation feedback, GSettings and fonts.
fn live_font_preserves_the_presented_character_grid() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-geometry",
        LiveFontCase::Geometry,
    );
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb, GSettings and Ubuntu Mono/DejaVu fonts.
fn gnu_live_font_overrides_current_fonts_and_sets_future_defaults() {
    check_live_font(
        GuiBackend::LinuxX11,
        "gnu-live-explicit-future",
        LiveFontCase::ExplicitAndFuture,
    );
}

#[test]
// Prerequisites: requires release binary/pdump, Weston, GSettings and Ubuntu Mono/DejaVu fonts.
fn live_font_overrides_current_fonts_and_sets_future_defaults() {
    check_live_font(
        GuiBackend::LinuxWayland,
        "live-explicit-future",
        LiveFontCase::ExplicitAndFuture,
    );
}

fn check_live_font(backend: GuiBackend, scenario: &str, case: LiveFontCase) {
    run_live_font(backend, scenario, case, None);
}

/// Run a resize-and-font case. The row supplies the end grids it accepts;
/// the fixture's stimulus never depends on which editor runs it.
fn check_live_font_grids(
    backend: GuiBackend,
    scenario: &str,
    case: LiveFontCase,
    grids: &[(u32, u32)],
) {
    let spec = grids
        .iter()
        .map(|(columns, lines)| format!("{columns}x{lines}"))
        .collect::<Vec<_>>()
        .join(" ");
    run_live_font(backend, scenario, case, Some(spec));
}

fn run_live_font(
    backend: GuiBackend,
    scenario: &str,
    case: LiveFontCase,
    expected_grids: Option<String>,
) {
    let root = neomacs_infra::crate_root!().join("../..");
    let artifacts = root.join("target/neomacs-gui-tests").join(scenario);
    let config = artifacts.join("settings-config");
    if config.exists() {
        fs::remove_dir_all(&config).unwrap();
    }
    fs::create_dir_all(&config).unwrap();
    let schemas = artifacts.join("schemas");
    fs::create_dir_all(&schemas).unwrap();
    fs::copy(
        root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.gschema.xml"),
        schemas.join("desktop-font-live.gschema.xml"),
    )
    .unwrap();
    assert!(
        Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status()
            .unwrap()
            .success()
    );
    let fontconfig = crate::desktop_font_pins::desktop_font_fontconfig(&artifacts);
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = match backend {
        GuiBackend::LinuxX11 => std::env::var_os("NEOMACS_GNU_GUI_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("emacs")),
        GuiBackend::LinuxWayland => std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs")),
        _ => unreachable!("live GSettings oracle runs on Linux"),
    };
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", fontconfig.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "keyfile")
    .with_env("NEOMACS_GUI_LIVE_FONT_CASE", case.as_ref())
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.to_string_lossy())
    .with_env("XDG_CONFIG_HOME", config.to_string_lossy());
    if let Some(grids) = expected_grids {
        plan = plan.with_env("NEOMACS_GUI_LIVE_FONT_EXPECT_GRIDS", grids);
    }
    if backend == GuiBackend::LinuxWayland {
        let receipt = artifacts.join("presentation.sexp");
        if receipt.exists() {
            fs::remove_file(&receipt).unwrap();
        }
        plan = plan.with_env(
            "NEOMACS_GUI_PRESENTATION_RECEIPT",
            receipt.to_string_lossy(),
        );
    }
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11").with_args([
            "-Q".to_string(),
            "--xrm".to_string(),
            "Xft.dpi: 96".to_string(),
            "-l".to_string(),
            root.join("crates/neomacs-gui-tests/fixtures/desktop-font-live.el")
                .to_string_lossy()
                .into_owned(),
        ]);
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .unwrap();
    let editor_state = neomacs_gui_tests::last_editor_state_line(&result.artifacts.stderr);
    assert!(
        !result.timed_out,
        "{result:#?}\nlast editor state: {editor_state}"
    );
    assert_eq!(
        result.exit_code,
        Some(0),
        "{result:#?}\nlast editor state: {editor_state}"
    );
    assert!(
        fs::read_to_string(&result.artifacts.stderr)
            .unwrap()
            .contains("LIVE-FONT-PASS"),
        "missing LIVE-FONT-PASS; last editor state: {editor_state}"
    );
}
