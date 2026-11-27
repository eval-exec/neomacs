//! #853: `:distant-foreground` on a GUI frame, GNU vs Neomacs, by painted
//! pixels.
//!
//! GNU contract (xfaces.c at the pinned commit): `load_face_colors`, which only
//! `realize_gui_face` calls, replaces the foreground with `:distant-foreground`
//! when `color_distance (bg, fg)` is below `face-near-same-color-threshold`
//! (30000 by default; src/xfaces.c:1417-1424, 4768-4789, 7749).
//!
//! The fixture paints three rows of glyphs on `#c6c7c7`, each an overlay face
//! over a text-property face as in the report:
//! - `#f0f0f0` with `:distant-foreground` `#1b2229`: distance 15486, so GNU
//!   paints `#1b2229`;
//! - `#f0f0f0` without it: GNU paints `#f0f0f0`;
//! - `#4078f2` with `:distant-foreground` `#1b2229`: distance 75282, so GNU
//!   keeps `#4078f2`.
//!
//! The row counts the pixels of each exact color in each editor's frame
//! image. GNU paints all three; Neomacs must paint the same set. Without the
//! distant swap the first row paints `#f0f0f0` and `#1b2229` is missing.
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

/// The glyph colors the fixture can paint, in the order the row reports them.
const GLYPH_COLORS: [(&str, [u8; 3]); 3] = [
    ("distant #1b2229", [0x1b, 0x22, 0x29]),
    ("own #f0f0f0", [0xf0, 0xf0, 0xf0]),
    ("own #4078f2", [0x40, 0x78, 0xf2]),
];

/// Antialiased glyphs still paint solid stems in their exact color; a row of
/// twenty capitals paints several hundred such pixels.
const PAINTED: usize = 100;

fn painted_glyph_colors(png: &Path, editor: &str) -> Vec<(&'static str, bool)> {
    let image = image::open(png)
        .unwrap_or_else(|error| panic!("{editor}: frame image {}: {error}", png.display()))
        .to_rgb8();
    GLYPH_COLORS
        .iter()
        .map(|(name, rgb)| {
            let count = image.pixels().filter(|pixel| pixel.0 == *rgb).count();
            (*name, count >= PAINTED)
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: &Path,
    session_env: &[(String, String)],
    editor: &str,
) -> Vec<(&'static str, bool)> {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/distant-foreground.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("distant-foreground", &fixture),
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
            // Explicit hang-avoidance cap only; the fixture exits after 1.5 s.
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: {result:#?}\nstderr:\n{stderr}"
    );
    assert_eq!(result.status, GuiRunStatus::Passed, "{editor}: {result:#?}");
    painted_glyph_colors(&result.artifacts.png, editor)
}

fn distant_foreground_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("distant-foreground")
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
    assert!(
        gnu.iter().all(|(_, painted)| *painted),
        "precondition: GNU paints all three glyph colors: {gnu:?}"
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
        "glyph colors painted [distant own-near own-far] differ from GNU 31.1"
    );
}

#[test]
fn distant_foreground_matches_gnu_on_x11() {
    distant_foreground_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn distant_foreground_matches_gnu_on_wayland() {
    distant_foreground_matches_gnu(GuiBackend::LinuxWayland);
}
