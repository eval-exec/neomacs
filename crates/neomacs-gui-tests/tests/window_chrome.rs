//! Native GUI smoke; does not substitute for AppKit visual QA.
use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn startup_global_tab_line_paints_scratch_visited_and_split_windows() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        Ok("x11") => GuiBackend::LinuxX11,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND to the native backend"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/issue-583-tab-line");
    let artifacts = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("issue-583-tab-line", fixture.join("init.el")),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_args([format!("--init-directory={}", fixture.display())]);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");

    for (tag, expected_buffers) in [
        ("scratch", vec!["*scratch*"]),
        ("visited", vec!["issue-583-visited.txt"]),
        ("split", vec!["issue-583-visited.txt", "*scratch*"]),
        ("flat-button", vec!["issue-583-visited.txt", "*scratch*"]),
    ] {
        let snapshot: serde_json::Value = serde_json::from_slice(
            &std::fs::read(format!(
                "{}.{tag}",
                result.artifacts.frame_snapshot_json.display()
            ))
            .unwrap(),
        )
        .unwrap();
        let frame = &snapshot["frames"][0];
        let image = image::open(format!(
            "{}.{tag}.png",
            result.artifacts.frame_snapshot_json.display()
        ))
        .unwrap()
        .to_rgba8();
        let scale_x = f64::from(image.width()) / frame["frame_pixel_width"].as_f64().unwrap();
        let scale_y = f64::from(image.height()) / frame["frame_pixel_height"].as_f64().unwrap();
        let bands = frame["frame_chrome"]["bands"].as_array().unwrap();
        assert!(!bands.is_empty(), "{tag}: exercise startup frame chrome");
        let chrome_bottom = bands.iter().fold(0.0_f64, |bottom, band| {
            bottom.max(
                band["bounds"]["y"].as_f64().unwrap() + band["bounds"]["height"].as_f64().unwrap(),
            )
        });
        let windows: Vec<_> = frame["window_infos"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|window| window["is_minibuffer"] == false)
            .collect();
        assert_eq!(windows.len(), expected_buffers.len(), "{tag}: {windows:?}");
        for buffer in expected_buffers {
            let window = windows
                .iter()
                .find(|window| window["buffer_name"] == buffer)
                .unwrap_or_else(|| panic!("{tag}: missing {buffer}: {windows:?}"));
            assert!(
                window["tab_line_height"].as_f64().unwrap() > 0.0,
                "{tag}: no tab-line geometry for {buffer}: {window}"
            );
            let matrix = frame["window_matrices"]
                .as_array()
                .unwrap()
                .iter()
                .find(|matrix| matrix["window_id"] == window["window_id"])
                .unwrap();
            let mut text = String::new();
            let mut flat_button_faces = 0;
            for row in matrix["matrix"]["rows"].as_array().unwrap() {
                if row["role"] != "TabLine" {
                    continue;
                }
                let tab_top = matrix["pixel_bounds"]["y"].as_f64().unwrap()
                    + row["pixel_y"].as_f64().unwrap();
                assert!(
                    tab_top >= chrome_bottom,
                    "{tag}: tab line for {buffer} is hidden behind frame chrome: {tab_top} < {chrome_bottom}"
                );
                let mut glyph_x = matrix["pixel_bounds"]["x"].as_f64().unwrap()
                    + row["pixel_x"].as_f64().unwrap();
                for glyph in row["glyphs"][1].as_array().unwrap() {
                    if let Some(ch) = glyph["glyph_type"]["Char"]["ch"].as_str() {
                        text.push_str(ch);
                    }
                    if tag == "flat-button" && window["selected"] == true {
                        let face = &frame["faces"][glyph["face_id"].to_string()];
                        if face["foreground"]["r"] == 1.0
                            && face["foreground"]["g"] == 0.0
                            && face["background"]["r"] == 0.0
                            && face["background"]["g"] == 1.0
                        {
                            flat_button_faces += 1;
                            assert_eq!(face["box_type"], "Line", "{face}");
                            assert_eq!(
                                face["box_color"], face["background"],
                                "flat-button without :color uses its background: {face}"
                            );
                            let px = ((glyph_x + glyph["pixel_width"].as_f64().unwrap() / 2.0)
                                * scale_x) as u32;
                            let py = ((tab_top + 0.5) * scale_y) as u32;
                            let pixel = image.get_pixel(px, py).0;
                            assert!(
                                pixel[0] < 20 && pixel[1] > 235 && pixel[2] < 20,
                                "flat-button top border must paint its green background at ({px}, {py}): {pixel:?}"
                            );
                        }
                    }
                    glyph_x += glyph["pixel_width"].as_f64().unwrap();
                }
            }
            assert!(
                text.contains(buffer),
                "{tag}: missing tab label {buffer}: {text:?}"
            );
            if tag == "flat-button" && window["selected"] == true {
                assert!(
                    flat_button_faces > 0,
                    "flat-button face must reach the displayed tab"
                );
            }
        }
    }
}

#[test]
// Prerequisites: requires a built binary/pdump and the requested native GUI backend.
fn native_chrome_theme_change_keeps_both_frames_alive() {
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        Ok("macos") => GuiBackend::Macos,
        Ok("windows") => GuiBackend::Windows,
        Ok("x11") => GuiBackend::LinuxX11,
        _ => panic!("set NEOMACS_GUI_TEST_BACKEND to the native backend"),
    };
    let root = neomacs_infra::crate_root!().join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "window-chrome",
            root.join("crates/neomacs-gui-tests/fixtures/window-chrome.el"),
        ),
    )
    .with_program(binary);
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&result.artifacts.gui_state).unwrap())
            .unwrap();
    assert_eq!(state["primary_alive"], true);
    assert_eq!(state["secondary_alive"], true);
    assert!(
        result.png_bytes.unwrap_or_default() > 0,
        "native surface readback artifact"
    );
}
