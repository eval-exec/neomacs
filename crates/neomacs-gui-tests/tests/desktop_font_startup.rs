use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunResult, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner, WaylandOutput, WestonDesktop, start_weston_with_desktop,
};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas, Ubuntu Mono, DejaVu and SVG packages"]
fn desktop_monospace_font_drives_initial_window_and_svg_metrics() {
    check_desktop_font_startup(WaylandOutput::Standard, "desktop-font-startup");
}

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas, Ubuntu Mono, DejaVu and SVG packages"]
fn hidpi_desktop_font_preserves_initial_and_resized_column_grid() {
    check_desktop_font_startup(WaylandOutput::HiDpi4k, "desktop-font-startup-hidpi");
}

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas and Ubuntu Mono"]
fn hidpi_scale_change_never_shrinks_the_logical_frame() {
    let result = check_fixture(
        WaylandOutput::HiDpi4k,
        "native-scale-startup",
        "native-scale-startup.el",
    );
    // Output scale discovery can happen before the Lisp fixture loads. Inspect
    // the real GUI-to-evaluator input boundary, including that startup interval.
    let trace = fs::read_to_string(result.artifacts.stdout).unwrap();
    let scale_two_resizes: Vec<_> = trace
        .lines()
        .filter(|line| {
            line.contains("input-bridge: received display event WindowResize")
                && line.contains("scale_factor: 2.0")
        })
        .collect();
    assert!(
        !scale_two_resizes.is_empty(),
        "must exercise native scale discovery"
    );
    for resize in scale_two_resizes {
        // Ubuntu Mono 13: 80 columns at 9 logical pixels plus 25px of chrome.
        assert!(
            resize.contains("width: 745,"),
            "scale discovery changed logical width: {resize}"
        );
    }
}

fn check_desktop_font_startup(output: WaylandOutput, scenario: &str) {
    check_fixture(output, scenario, "desktop-font-startup.el");
}

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas and Ubuntu Mono"]
fn hidpi_native_configure_preserves_the_requested_column_grid() {
    check_fixture(
        WaylandOutput::HiDpi4k,
        "native-resize-increments-hidpi",
        "native-resize-increments.el",
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston, glib-compile-schemas and Ubuntu Mono"]
fn hidpi_decorated_fullscreen_restores_the_requested_column_grid() {
    check_fixture(
        WaylandOutput::HiDpi4k,
        "native-resize-decorations-hidpi",
        "native-resize-decorations.el",
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston and Ubuntu Mono"]
fn hidpi_8k_decorated_fullscreen_restores_the_requested_column_grid() {
    check_fixture(
        WaylandOutput::HiDpi8k,
        "native-resize-decorations-8k",
        "native-resize-decorations.el",
    );
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback and Ubuntu Mono"]
fn hidpi_8k_fullscreen_after_confirmed_presentation() {
    let result = check_fixture(
        WaylandOutput::HiDpi8k,
        "native-resize-presented-8k",
        "native-resize-presented.el",
    );
    check_receipt_timestamp(&result);
}

#[test]
#[ignore = "requires release binary/pdump, Weston presentation feedback and Ubuntu Mono"]
fn hidpi_8k_slow_desktop_fullscreen_after_confirmed_presentation() {
    let result = check_fixture_with_desktop(
        WaylandOutput::HiDpi8k,
        "native-resize-presented-slow-8k",
        "native-resize-presented.el",
        WestonDesktop::DefaultPattern,
    );
    check_receipt_timestamp(&result);
}

fn check_receipt_timestamp(result: &GuiRunResult) {
    let receipt_path = result
        .artifacts
        .stdout
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("presentation.sexp");
    let receipt = fs::read_to_string(receipt_path).unwrap();
    let words: Vec<_> = receipt
        .trim()
        .trim_matches(['(', ')'])
        .split_whitespace()
        .collect();
    let field = |name: &str| -> u64 {
        words
            .windows(2)
            .find(|pair| pair[0] == name)
            .unwrap_or_else(|| panic!("missing {name} in {receipt}"))[1]
            .parse()
            .unwrap()
    };
    let seconds = field(":seconds");
    let timestamp = format!(
        ".presented({}, {}, {},",
        seconds >> 32,
        seconds & 0xffff_ffff,
        field(":nanoseconds")
    );
    let clock = format!(".clock_id({})", field(":clock-id"));
    let wire = fs::read_to_string(&result.artifacts.stderr).unwrap();
    // Compare against independent native protocol traffic, not the renderer's
    // own confirmation log or a timestamp recomputed from its scheduler clock.
    assert!(
        wire.lines()
            .any(|line| line.contains("wp_presentation_feedback") && line.contains(&timestamp)),
        "receipt timestamp absent from native Presented events: {receipt}"
    );
    assert!(
        wire.lines()
            .any(|line| line.contains("wp_presentation") && line.contains(&clock)),
        "receipt clock absent from native ClockId events: {receipt}"
    );
}

fn check_fixture(output: WaylandOutput, scenario: &str, fixture: &str) -> GuiRunResult {
    check_fixture_with_desktop(output, scenario, fixture, WestonDesktop::Solid)
}

fn check_fixture_with_desktop(
    output: WaylandOutput,
    scenario: &str,
    fixture: &str,
    desktop: WestonDesktop,
) -> GuiRunResult {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = root.join("target/neomacs-gui-tests").join(scenario);
    let receipt = artifacts.join("presentation.sexp");
    match fs::remove_file(&receipt) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove previous presentation receipt: {error}"),
    }
    let schemas = artifacts.join("desktop-font-startup-schemas");
    fs::create_dir_all(&schemas).unwrap();
    fs::copy(
        root.join("crates/neomacs-gui-tests/fixtures/desktop-fonts.gschema.xml"),
        schemas.join("desktop-fonts.gschema.xml"),
    )
    .unwrap();
    assert!(
        Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status()
            .unwrap()
            .success()
    );
    let backend = GuiBackend::LinuxWayland;
    let session = start_weston_with_desktop(&artifacts, output, desktop).unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            scenario,
            root.join("crates/neomacs-gui-tests/fixtures").join(fixture),
        ),
    )
    .with_program(binary)
    .with_env("WAYLAND_DEBUG", "client")
    .with_env(
        "NEOMACS_GUI_PRESENTATION_RECEIPT",
        receipt.to_string_lossy(),
    )
    // Scale startup assertions include the public GUI input stream before
    // Lisp loads. Do not let the invoking shell silently disable that trace.
    .with_env("RUST_LOG", "debug")
    .with_env("GSETTINGS_SCHEMA_DIR", schemas.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "memory");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    for key in ["NEOMACS_GUI_SVG_LIB_DIR", "NEOMACS_GUI_SVG_TAG_MODE_DIR"] {
        if let Ok(value) = std::env::var(key) {
            plan = plan.with_env(key, value);
        }
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    result
}
