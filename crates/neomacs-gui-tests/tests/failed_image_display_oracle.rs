//! Attested GNU Emacs 31.1 GUI oracle for failed-image display geometry,
//! measurement (`window-text-pixel-size` / `posn-at-point`), and hollow-box
//! rendering (`src/image.c:3568-3583`, `src/xterm.c:10283-10286`, `6737-6752`).
//!
//! All test GIF data is generated in code into a tempdir (never tracked).

#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const RED: [u8; 3] = [0xff, 0x00, 0x00];
const GREEN: [u8; 3] = [0x00, 0xff, 0x00];
const BLUE: [u8; 3] = [0x00, 0x00, 0xff];
const YELLOW: [u8; 3] = [0xff, 0xff, 0x00];
const MAGENTA: [u8; 3] = [0xff, 0x00, 0xff];
const ORANGE: [u8; 3] = [0xff, 0x80, 0x00];
const SKY: [u8; 3] = [0x00, 0x80, 0xff];

/// Build a 48x48 GIF whose image descriptor extends outside the logical screen
/// (left=48, width=1 -> right=49 > 48). Its header probes cleanly as 48x48,
/// then decode fails; layout and measurement must collapse to GNU's 30x30
/// failed-image box rather than keeping the 48x48 provisional slot.
fn build_oob_48x48_gif() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&48u16.to_le_bytes());
    out.extend_from_slice(&48u16.to_le_bytes());
    out.push(0x80); // GCT with 2 entries
    out.push(0);
    out.push(0);
    out.extend_from_slice(&[0xff, 0x80, 0x00, 0x00, 0x00, 0x00]);
    // 1x1 subimage placed at left=48 (outside 48x48 screen) with valid 1-pixel LZW.
    out.push(0x2c);
    out.extend_from_slice(&48u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.push(0x00);
    out.extend_from_slice(&[0x02, 0x02, 0x44, 0x01, 0x00]);
    out.push(0x3b);
    out
}

fn color_bbox_and_count(img: &image::RgbaImage, rgb: [u8; 3]) -> (usize, Option<(u32, u32)>) {
    let mut count = 0usize;
    let mut min_x = u32::MAX;
    let mut max_x = 0u32;
    let mut min_y = u32::MAX;
    let mut max_y = 0u32;
    for (x, y, p) in img.enumerate_pixels() {
        if [p[0], p[1], p[2]] == rgb {
            count += 1;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    let dims = (count > 0).then(|| (max_x - min_x + 1, max_y - min_y + 1));
    (count, dims)
}

#[test]
fn failed_image_display_geometry_and_hollow_box_match_gnu() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/failed-image-display-{}", std::process::id()));
    let img_dir = artifacts.join("images");
    fs::create_dir_all(&img_dir).unwrap();
    fs::write(img_dir.join("oob-48x48.gif"), build_oob_48x48_gif()).unwrap();

    let fixture =
        root.join("crates/neomacs-gui-tests/fixtures/failed-image-display-oracle/init.el");

    // 1. Attested GNU Emacs 31.1 oracle run (NEOVM_FORCE_ORACLE_PATH required).
    let gnu_program = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the checked GNU GUI oracle"),
    );
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let gnu_state_path = artifacts.join("gnu-state.json");
    let gnu_png_path = artifacts.join("gnu-export.png");
    let mut gnu_cmd = CommandSpec {
        program: gnu_program,
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_FAILED_IMAGE_DIR".into(),
                img_dir.display().to_string(),
            ),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state_path.display().to_string(),
            ),
            (
                "NEOMACS_GNU_EXPORT_PNG".into(),
                gnu_png_path.display().to_string(),
            ),
        ],
    };
    gnu_cmd.env.extend(gnu_session.env().iter().cloned());
    let gnu_output = ProcessGuiCommandRunner
        .run(
            &gnu_cmd,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!gnu_output.timed_out, "GNU: {gnu_output:#?}");
    assert_eq!(gnu_output.exit_code, Some(0), "GNU: {gnu_output:#?}");
    let gnu: serde_json::Value =
        serde_json::from_slice(&fs::read(&gnu_state_path).unwrap()).unwrap();
    assert_eq!(gnu["native-engine"], serde_json::json!(false), "{gnu}");
    assert_eq!(gnu["graphic"], serde_json::json!(true), "{gnu}");
    assert_eq!(gnu["sz-missing"], serde_json::json!([30, 30]), "{gnu}");
    assert_eq!(gnu["wtps-missing"], serde_json::json!([30, 30]), "{gnu}");
    assert_eq!(gnu["wtps-margined"], serde_json::json!([30, 30]), "{gnu}");
    assert_eq!(gnu["wtps-sized"], serde_json::json!([40, 36]), "{gnu}");
    assert_eq!(gnu["wtps-oob"], serde_json::json!([30, 30]), "{gnu}");
    assert_eq!(gnu["adv-missing"], serde_json::json!(30), "{gnu}");
    assert_eq!(gnu["adv-margined"], serde_json::json!(30), "{gnu}");
    assert_eq!(gnu["adv-sized"], serde_json::json!(40), "{gnu}");
    assert_eq!(gnu["adv-oob"], serde_json::json!(30), "{gnu}");

    let gnu_pixels = image::open(&gnu_png_path)
        .expect("GNU x-export-frames PNG must exist")
        .to_rgba8();

    // 2. Neomacs run: captures both Elisp state and surface readback PNG.
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
        GuiScenario::new("failed-image-display-oracle", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_FAILED_IMAGE_DIR", img_dir.display().to_string())
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
    assert_eq!(state["native-engine"], serde_json::json!(true), "{state}");
    assert_eq!(state["graphic"], serde_json::json!(true), "{state}");
    for key in [
        "sz-missing",
        "sz-margined",
        "sz-sized",
        "sz-oob",
        "wtps-missing",
        "wtps-margined",
        "wtps-sized",
        "wtps-oob",
        "adv-missing",
        "adv-margined",
        "adv-sized",
        "adv-oob",
    ] {
        assert_eq!(
            state[key], gnu[key],
            "failed-image display/measurement mismatch for {key}: neo={state} gnu={gnu}"
        );
    }

    let neo_pixels = image::open(&result.artifacts.png)
        .expect("Neomacs readback PNG exists")
        .to_rgba8();

    // Direct GNU vs Neomacs pixel parity on every hollow box outline and interior:
    // - 30x30 hollow 1px box: 116 outline pixels (30+30+28+28), 784 interior pixels (28*28)
    // - 40x36 hollow 1px box: 148 outline pixels (40+40+34+34)
    let checks = [
        ("missing 30x30 outline", RED, (116, Some((30, 30)))),
        ("missing 30x30 interior", GREEN, (784, Some((28, 28)))),
        ("margined 30x30 outline", BLUE, (116, Some((30, 30)))),
        ("margined 30x30 interior", YELLOW, (784, Some((28, 28)))),
        ("sized 40x36 outline", MAGENTA, (148, Some((40, 36)))),
        ("oob-48x48->30x30 outline", ORANGE, (116, Some((30, 30)))),
        ("oob-48x48->30x30 interior", SKY, (784, Some((28, 28)))),
    ];
    for (label, rgb, expected) in checks {
        let gnu_got = color_bbox_and_count(&gnu_pixels, rgb);
        let neo_got = color_bbox_and_count(&neo_pixels, rgb);
        assert_eq!(gnu_got, expected, "GNU oracle failed-box check ({label})");
        assert_eq!(
            neo_got, gnu_got,
            "Neomacs failed-box mismatch vs GNU ({label}): neo={neo_got:?} gnu={gnu_got:?}"
        );
    }
}
