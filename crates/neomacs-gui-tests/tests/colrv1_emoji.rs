//! Issue #542: emoji from a COLRv1 color font must be *drawn*, not just
//! selected.
//!
//! The reporter's `font-at` output was identical between GNU and neomacs
//! (`Noto Color Emoji`), and only the pixels differed: GNU drew the emoji on
//! macOS (CoreText) while neomacs left the cells blank.  This test therefore
//! asserts on the surface readback's per-cell pixel averages, not on font
//! selection.
//!
//! Only the selection half has a GNU oracle.  GNU contains no COLR/CBDT/sbix
//! code at all (no `FT_LOAD_COLOR` anywhere in its history, verified upstream),
//! and its X11/Xft backend can only delegate to Xft, so a Linux GNU built
//! without cairo cannot paint these cells even though it selects the same
//! face.  GNU on macOS paints them through CoreText, which this environment
//! cannot run; GNU on Linux would need its cairo path (`ftcrfont_draw` ->
//! `cairo_show_glyphs`), which this oracle binary is not built with.  The pixel
//! assertions below are therefore anchored to the artwork's own colors, and the
//! font-selection assertions are compared against GNU directly.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

/// Average RGBA of the readback box reported for `ch`, if it was reported.
///
/// The readback line carries several other `avg=` fields (frame bands) before
/// the glyph box, so the search starts at the box itself.
fn glyph_box_average(result: &neomacs_gui_tests::GuiRunResult, ch: char) -> Option<[f32; 4]> {
    let needle = format!("glyph_box='{ch}'");
    let line = result
        .readback_diagnostics
        .iter()
        .find(|line| line.contains(&needle))?;
    let boxed = &line[line.find(&needle)?..];
    let average = boxed.split("avg=(").nth(1)?.split(')').next()?;
    let channels: Vec<f32> = average
        .split(',')
        .filter_map(|channel| channel.trim().parse().ok())
        .collect();
    assert_eq!(channels.len(), 4, "unparsable readback line: {line}");
    Some([channels[0], channels[1], channels[2], channels[3]])
}

#[test]
fn colrv1_emoji_cells_are_painted_in_color() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/colrv1-emoji-{}", std::process::id()));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::spleen_2_2_0().otb(),
        fonts.join("spleen.otb"),
    )
    .unwrap();
    // The reporter's font: COLR version 1 with empty outlines under its emoji.
    fs::copy(
        neomacs_test_fonts::noto_color_emoji_colrv1(),
        fonts.join("emoji.ttf"),
    )
    .unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Spleen</family></prefer></alias></fontconfig>",
            fonts.display(),
            artifacts.join("fontcache").display()
        ),
    )
    .unwrap();

    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts.join("native"))
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/colrv1-emoji/init.el");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("colrv1-emoji", &fixture),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    // The harness points the readback PNG at its own artifact path; only the
    // readback itself has to be switched on here.
    // The fixture draws during startup, so the first frames already carry the
    // cells: a small window keeps the requested PNG inside the run's lifetime
    // without racing the drawing.
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "4");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert!(!result.timed_out);
    assert_eq!(result.exit_code, Some(0));

    // Font selection was never the bug: both editors already picked the emoji
    // face for these cells, and GNU is the oracle for that half.
    let measurements = result
        .gui_state
        .as_ref()
        .expect("the fixture writes its state file");
    assert_eq!(
        measurements["grape-font"], "Noto Color Emoji",
        "the emoji face must be selected: {measurements}"
    );
    assert_eq!(measurements["apple-font"], "Noto Color Emoji");

    let gnu_state = artifacts.join("gnu-state.json");
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let mut gnu = CommandSpec {
        program: std::env::var_os("NEOMACS_GUI_TEST_GNU_EMACS")
            .or_else(|| std::env::var_os("NEOMACS_GNU_EMACS_BINARY"))
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("FONTCONFIG_FILE".into(), config.display().to_string()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state.display().to_string(),
            ),
        ],
    };
    gnu.env.extend(gnu_session.env().iter().cloned());
    let gnu_output = ProcessGuiCommandRunner
        .run(
            &gnu,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!gnu_output.timed_out, "GNU: {gnu_output:#?}");
    let gnu: serde_json::Value =
        serde_json::from_slice(&fs::read(&gnu_state).expect("GNU writes the same state file"))
            .unwrap();
    assert_eq!(gnu["native-engine"], false, "GNU must be the GNU run");
    assert_eq!(
        gnu["grape-font"], measurements["grape-font"],
        "font selection must agree with GNU for the same fonts: {gnu}"
    );
    assert_eq!(gnu["apple-font"], measurements["apple-font"]);

    // Cell averages are diluted by the background around the artwork, so the
    // assertions are on the hue the artwork pushes the average toward.  A
    // blank cell averages to the gray frame background (all channels equal); a
    // monochrome fallback would average to the cell's blue foreground (red
    // and green equal, blue far above both).  Neither can satisfy a purple or
    // a red hue, and the two hues cannot satisfy each other.
    let grape = glyph_box_average(&result, '\u{1F347}')
        .unwrap_or_else(|| panic!("readback reported no box for the grape: {result:#?}"));
    assert!(
        grape[0] > grape[1] + 10.0 && grape[2] > grape[1] + 10.0,
        "grape cell is not painted purple: {grape:?}"
    );
    let apple = glyph_box_average(&result, '\u{1F34E}')
        .unwrap_or_else(|| panic!("readback reported no box for the apple: {result:#?}"));
    assert!(
        apple[0] > apple[1] + 40.0 && apple[0] > apple[2] + 40.0,
        "apple cell is not painted red: {apple:?}"
    );
}
