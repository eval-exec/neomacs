//! XPM named colors must resolve through the editor's own color hook.
//!
//! GNU hands an XPM `c` value to the frame terminal's `defined_color_hook`
//! (src/image.c:6503-6511) -- the same hook `color-values' reports -- whose
//! name database is the X11 `rgb.txt` data on every platform. The fixture
//! paints one swatch per name from the issue #545 report and records what each
//! editor says the name means; the readback PNG says what it rendered as. GNU
//! supplies the oracle values, the editor under test must both agree with them
//! and have painted them.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

/// `etc/rgb.txt` for the names in the report. `green` and `maroon` are pinned
/// against their CSS values (#008000 / #800000), `grayNN` against the ladder
/// XPM artwork shades details with.
const CASES: [(&str, [u8; 3]); 6] = [
    ("gray14", [0x24, 0x24, 0x24]),
    ("gray50", [0x7f, 0x7f, 0x7f]),
    ("gray75", [0xbf, 0xbf, 0xbf]),
    ("green", [0x00, 0xff, 0x00]),
    ("maroon", [0xb0, 0x30, 0x60]),
    ("light blue", [0xad, 0xd8, 0xe6]),
];

/// One 48x48 swatch is 2304 pixels; allow for the odd blend at its edges.
const PAINTED_PIXEL_FLOOR: usize = 1500;

/// The colours the two fallback swatches ask for and must not get: one face
/// asks for red, one specification asks for blue (issue #550).
const FALLBACK_FACE_DECOY: [u8; 3] = [0xff, 0x00, 0x00];
const FALLBACK_SPEC_DECOY: [u8; 3] = [0x00, 0x00, 0xff];

/// The decoys appear nowhere else in the frame, so anything near a full swatch
/// means the decoy won.
const DECOY_PIXEL_CEILING: usize = 100;

/// GNU and Neomacs both answer `color-values' in 16-bit channels; the renderer
/// keeps the most-significant 8 bits (257 == 0x0101).
fn rgb16_to_rgb8(value: &serde_json::Value) -> Option<[u8; 3]> {
    let channels = value.as_array()?;
    let channel = |index: usize| Some((channels.get(index)?.as_u64()? / 257) as u8);
    Some([channel(0)?, channel(1)?, channel(2)?])
}

#[test]
fn xpm_named_colors_resolve_like_gnu_and_paint_that_way() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/xpm-named-colors-{}", std::process::id()));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/xpm-named-colors/init.el");

    // GNU first: its `color-values' on a graphic frame is the oracle for what
    // each name means, and it is the same hook its XPM loader calls.
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let gnu_state = artifacts.join("gnu-state.json");
    let mut command = CommandSpec {
        program: std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| "emacs".into()),
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state.display().to_string(),
            ),
        ],
    };
    command.env.extend(gnu_session.env().iter().cloned());
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!output.timed_out, "GNU: {output:#?}");
    let gnu: serde_json::Value = serde_json::from_slice(&fs::read(&gnu_state).unwrap()).unwrap();
    assert_eq!(
        gnu["native-engine"],
        serde_json::json!(false),
        "the oracle run must be GNU Emacs: {gnu}"
    );
    assert_eq!(
        gnu["graphic"],
        serde_json::json!(true),
        "GNU needs a graphic frame for its color hook: {gnu}"
    );
    for (name, expected) in CASES {
        assert_eq!(
            rgb16_to_rgb8(&gnu[name]),
            Some(expected),
            "GNU color-values for {name}: {gnu}"
        );
    }

    // The editor under test: the same fixture, and the swatches it paints must
    // carry exactly those colors.
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
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("xpm-named-colors", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
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
    assert_eq!(result.exit_code, Some(0));

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(
        state["native-engine"],
        serde_json::json!(true),
        "the run under test must be the Rust engine: {state}"
    );
    assert_eq!(
        state["graphic"],
        serde_json::json!(true),
        "the swatches need a graphic frame: {state}"
    );
    for (name, expected) in CASES {
        assert_eq!(
            rgb16_to_rgb8(&state[name]),
            Some(expected),
            "color-values for {name}: {state}"
        );
    }

    let png = image::open(&result.artifacts.png).expect("the readback PNG exists");
    let pixels = png.to_rgba8();
    for (name, expected) in CASES {
        let painted = count_pixels(&pixels, expected);
        assert!(
            painted >= PAINTED_PIXEL_FLOOR,
            "{name}: {painted} pixels of {expected:?} in the frame; \
             the swatch is missing or rendered another color"
        );
    }

    // Issue #550: a key that resolves to nothing is painted by a rule, and the
    // rule reads the frame's foreground (GNU FRAME_FOREGROUND_PIXEL, read once
    // at src/image.c:6518 and used at :6537-6538) -- never the face the image
    // is displayed under, never the specification's :foreground.
    let frame_foreground = rgb16_to_rgb8(&state["frame-foreground"])
        .expect("the fixture records the frame foreground");
    assert_eq!(
        state["default-face-foreground"], state["frame-foreground"],
        "GNU keeps the frame's foreground-color and the default face's foreground equal: {state}"
    );
    for (label, decoy) in [
        ("fallback-face", FALLBACK_FACE_DECOY),
        ("fallback-spec", FALLBACK_SPEC_DECOY),
    ] {
        let decoy_painted = count_pixels(&pixels, decoy);
        assert!(
            decoy_painted < DECOY_PIXEL_CEILING,
            "{label}: {decoy_painted} pixels of {decoy:?}; the swatch followed the \
             {label} colour instead of the frame foreground"
        );
        let painted = count_pixels(&pixels, frame_foreground);
        assert!(
            painted >= PAINTED_PIXEL_FLOOR,
            "{label}: {painted} pixels of the frame foreground {frame_foreground:?} in the frame"
        );
    }
}

fn count_pixels(pixels: &image::RgbaImage, colour: [u8; 3]) -> usize {
    pixels
        .pixels()
        .filter(|pixel| {
            let [red, green, blue, _] = pixel.0;
            [red, green, blue] == colour
        })
        .count()
}
