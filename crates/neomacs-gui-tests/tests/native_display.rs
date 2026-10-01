use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunResult, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
#[path = "native_display/linux_scroll.rs"]
mod linux_scroll;

#[test]
// Prerequisites: requires a fresh release binary/pdump and a native graphical session.
fn native_startup_font_and_resize_contract() {
    let result = run_native_contract("native-display", "native-display-contract.el", false);
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["columns"], 91);
    assert_eq!(state["contract"], "native-display");
    let snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.frame_snapshot_json).unwrap()).unwrap();
    assert!(snapshot.is_object());
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    assert!(pixels.width() > 0 && pixels.height() > 0);
    let first = pixels.get_pixel(0, 0);
    assert!(
        pixels.pixels().any(|pixel| pixel != first),
        "GUI readback is blank"
    );
}

#[test]
// Canonical commands and native rendering; physical device transport is tested separately.
fn native_rich_scroll_commands_preserve_pixel_offset() {
    #[cfg(target_os = "linux")]
    for result in linux_scroll::run_rich_pixel_contracts() {
        assert_native_scroll_contract(&result);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let result =
            run_native_contract("native-display-scroll", "native-scroll-contract.el", false);
        assert_native_scroll_contract(&result);
    }
}

fn assert_native_scroll_contract(result: &GuiRunResult) {
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["contract"], "native-scroll");
    assert_eq!(state["content"]["lines"], 100_000);
    assert_eq!(state["content"]["face-variants"], 6);
    assert_eq!(
        state["content"]["font-families"].as_array().unwrap().len(),
        3
    );
    assert_eq!(state["content"]["font-selection"], "installed");
    assert!(state["content"]["overlays"].as_u64().unwrap() >= 12_500);
    assert_eq!(state["pixel-down"][0], state["pixel-origin"][0]);
    assert_eq!(
        state["pixel-down"][1].as_i64().unwrap() - state["pixel-origin"][1].as_i64().unwrap(),
        7
    );
    assert_eq!(state["pixel-returned"], true);
    assert_eq!(state["page-advanced"], true);
    assert_eq!(state["page-returned"], true);
    assert!(
        fs::metadata(&result.artifacts.frame_snapshot_json)
            .unwrap()
            .len()
            < 64 * 1024,
        "geometry snapshot unexpectedly contains bulky replay data"
    );
    let snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.frame_snapshot_json).unwrap()).unwrap();
    assert!(
        snapshot["frames"].as_array().unwrap().iter().any(|frame| {
            frame["window_infos"]
                .as_array()
                .unwrap()
                .iter()
                .any(|window| {
                    // Protocol offsets are zero-based; Lisp positions start at one.
                    window["window_start"]
                        .as_u64()
                        .and_then(|start| start.checked_add(1))
                        == state["final-start"].as_u64()
                        && window["buffer_size"]
                            .as_u64()
                            .is_some_and(|size| size > 1_000_000)
                })
        }),
        "native snapshot did not contain the final rich viewport"
    );
    let pixels = image::open(&result.artifacts.png).unwrap().to_rgba8();
    let first = pixels.get_pixel(0, 0);
    assert!(
        pixels.pixels().any(|pixel| pixel != first),
        "scroll readback is blank"
    );
    assert!(
        pixels
            .pixels()
            .any(|pixel| { pixel[0].abs_diff(pixel[2]) > 60 || pixel[1].abs_diff(pixel[2]) > 60 }),
        "readback did not contain the rich buffer's colored faces"
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
// Prerequisites: requires fresh release binary/pdump and resource font installed on an ephemeral CI runner.
fn native_resource_font_drives_first_window() {
    assert!(
        std::env::var("NEOMACS_GUI_RESOURCE_FAMILY").is_ok(),
        "resource fixture is not configured"
    );
    let result = run_native_contract("native-display-resource", "native-resource-font.el", true);
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    let trace = fs::read_to_string(&result.artifacts.stdout).unwrap();
    let first = trace
        .lines()
        .find_map(|line| {
            line.split_once("creating primary window: emacs_pixels=")
                .map(|(_, size)| size)
        })
        .expect("missing initial native allocation");
    let (width, height) = first.split_once('x').unwrap();
    assert_eq!(
        width.parse::<u64>().unwrap(),
        state["native_width"].as_u64().unwrap(),
        "resource must drive the first allocation"
    );
    assert_eq!(
        height
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        state["native_height"].as_u64().unwrap()
    );
}

fn run_native_contract(name: &str, fixture: &str, resources: bool) -> GuiRunResult {
    let root = neomacs_infra::workspace_root();
    let backend = if cfg!(target_os = "macos") {
        GuiBackend::Macos
    } else if cfg!(windows) {
        GuiBackend::Windows
    } else {
        GuiBackend::LinuxWayland
    };
    // A unique directory prevents any prior PNG or state file from passing.
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("{name}-{}", std::process::id()));
    assert!(!artifacts.exists());
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(if cfg!(windows) {
                "target/release/neomacs.exe"
            } else {
                "target/release/neomacs"
            })
        });
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            name,
            root.join("crates/neomacs-gui-tests/fixtures").join(fixture),
        ),
    )
    .with_program(binary)
    .with_env("RUST_LOG", "info");
    if name == "native-display-scroll" {
        // Keep readback active until the rich fixture and its scrolls render.
        plan = plan.with_env("NEOMACS_DEBUG_SURFACE_READBACK", "100");
    }
    if resources {
        plan = plan.with_args(vec![
            "--no-init-file".into(),
            "--no-site-file".into(),
            "--no-site-lisp".into(),
            "--no-splash".into(),
            "--no-desktop".into(),
            "-l".into(),
            root.join("crates/neomacs-gui-tests/fixtures")
                .join(fixture)
                .display()
                .to_string(),
        ]);
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
    result
}
