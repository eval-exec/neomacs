use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunResult, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

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
