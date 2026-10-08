//! A CJK script fontset rule must select real glyphs independently of metadata repertory.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_melpa_test_support::EmacsRuntime;
use std::{
    fs, thread,
    time::{Duration, Instant},
};

#[test]
fn han_fontset_rule_with_absent_registry_selects_same_face_as_unicode_registry() {
    let root = neomacs_infra::workspace_root();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let control = root.join(format!(
        "tmp/fontset-han-gui-{}-{run_id}",
        std::process::id()
    ));
    fs::create_dir_all(&control).unwrap();
    let gnu = EmacsRuntime::gnu_emacs();
    let fonts = control.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::mplus_1_code_thin(),
        fonts.join("MPLUS1Code-Thin.ttf"),
    )
    .unwrap();
    fs::copy(
        neomacs_test_fonts::spleen_2_2_0().otb(),
        fonts.join("spleen-8x16.otb"),
    )
    .unwrap();
    fs::copy(
        neomacs_test_fonts::lxgw_wenkai_nerd_regular_1_522(),
        fonts.join("LXGWWenKaiNerdFont-Regular.ttf"),
    )
    .unwrap();
    let config = control.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>sans-serif</family><prefer><family>M PLUS 1 Code</family></prefer></alias><alias><family>monospace</family><prefer><family>M PLUS 1 Code</family></prefer></alias><match target=\"pattern\"><test name=\"family\" qual=\"any\"><string>Spleen</string></test><edit name=\"family\" mode=\"append\"><string>M PLUS 1 Code</string></edit></match></fontconfig>",
            fonts.display(),
            control.join("fontcache").display()
        ),
    )
    .unwrap();
    let script = control.join("fixture.el");
    fs::write(
        &script,
        include_str!("../fixtures/fontset-han-repertory.el"),
    )
    .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Ok("x11" | "linux-x11") | Err(_) => GuiBackend::LinuxX11,
        Ok(other) => panic!("unsupported GUI backend {other}"),
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&control)
        .unwrap();
    let gnu_control = control.join("GNU");
    fs::create_dir_all(&gnu_control).unwrap();
    // The pinned GNU reference uses GTK/X11. Keep its display owned by this
    // test even when the Neomacs presentation is hosted by Weston.
    let gnu_session = (backend == GuiBackend::LinuxWayland)
        .then(|| DisplayHarness::for_backend(GuiBackend::LinuxX11).start_session(&gnu_control))
        .transpose()
        .unwrap();
    let mut command = gnu.command();
    command
        .args(["-Q", "-l"])
        .arg(&script)
        .env("NEOMACS_HAN_CONTROL", &gnu_control)
        .env("FONTCONFIG_FILE", &config)
        .env("TMPDIR", &control)
        .envs(
            gnu_session
                .as_ref()
                .unwrap_or(&session)
                .env()
                .iter()
                .cloned(),
        );
    let output =
        neomacs_melpa_test_support::output_with_timeout(&mut command, Duration::from_secs(30))
            .unwrap();
    fs::write(gnu_control.join("stdout"), &output.stdout).unwrap();
    fs::write(gnu_control.join("stderr"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "GNU reference failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = serde_json::json!({"passed": true, "baseline-file": "MPLUS1Code-Thin.ttf", "absent-registry-file": "LXGWWenKaiNerdFont-Regular.ttf", "explicit-registry-file": "LXGWWenKaiNerdFont-Regular.ttf"});
    let gnu_result: serde_json::Value =
        serde_json::from_slice(&fs::read(gnu_control.join("result.json")).unwrap()).unwrap();
    assert_eq!(gnu_result, expected);
    let paths = GuiArtifactSet::new(&control, backend, "fontset-han-repertory");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &control,
        GuiScenario::new("fontset-han-repertory", &script),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "warn")
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("TMPDIR", control.display().to_string())
    .with_env("NEOMACS_HAN_CONTROL", control.display().to_string());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let (run, presentation) = thread::scope(|scope| {
        let run = scope.spawn(|| {
            plan.run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(120)),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(115);
        let mut presentation =
            Err("Han fontset selection did not produce a green presentation".to_owned());
        while !run.is_finished() && Instant::now() < deadline {
            if let Ok(error) = fs::read_to_string(control.join("error.el")) {
                presentation = Err(format!("public fontset/selected-glyph error: {error}"));
                break;
            }
            if let Ok(bytes) = fs::read(control.join("result.json"))
                && let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && state["passed"] == false
            {
                fs::write(control.join("painted"), "failed").unwrap();
                presentation = Err(format!("Han fontset selection failed: {state}"));
                break;
            }
            if control.join("result.json").exists()
                && let Ok(png) = image::open(&paths.png)
            {
                let green = png
                    .to_rgba8()
                    .pixels()
                    .filter(|pixel| {
                        let [r, g, b, _] = pixel.0;
                        g > 240 && r < 15 && b < 15
                    })
                    .count();
                if green > 3000 {
                    fs::write(control.join("painted"), "painted").unwrap();
                    presentation = Ok(());
                    break;
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        (run.join().unwrap(), presentation)
    });
    assert!(
        presentation.is_ok(),
        "{presentation:?}; artifacts {control:?}"
    );
    let result = run.unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(control.join("result.json")).unwrap()).unwrap();
    assert_eq!(state["passed"], true, "Han fontset selection: {state}");
    assert_eq!(
        state, expected,
        "public selected-glyph font files must match GNU"
    );
    let snapshot = fs::read_to_string(control.join("final.json")).unwrap();
    assert!(
        snapshot.contains("*fontset-han-repertory*"),
        "successful fontset selection must paint its buffer"
    );
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    let mut rendered_han = 0;
    for frame in snapshot["frames"].as_array().expect("snapshot frames") {
        for window in frame["window_matrices"]
            .as_array()
            .expect("window matrices")
        {
            for row in window["matrix"]["rows"].as_array().expect("glyph rows") {
                for area in row["glyphs"].as_array().expect("row glyph areas") {
                    for glyph in area.as_array().expect("area glyphs") {
                        if glyph["glyph_type"]["Char"]["ch"].as_str() != Some("中") {
                            continue;
                        }
                        rendered_han += 1;
                        let face_id = glyph["face_id"]
                            .as_u64()
                            .expect("glyph face id")
                            .to_string();
                        let selected = frame["char_fonts"]
                            .get(&face_id)
                            .and_then(|face| face.get("中"))
                            .expect("rendered CJK glyph must carry its resolved font");
                        assert!(
                            selected["glyph_id"].as_u64().is_some_and(|id| id > 0),
                            "rendered CJK glyph must not use the missing-glyph id: {selected}"
                        );
                        let font_id = selected["resolved_font_id"]
                            .as_u64()
                            .expect("resolved font id")
                            .to_string();
                        let file = frame["fonts"]
                            .get(&font_id)
                            .and_then(|font| font["identity"]["file_path"].as_str())
                            .expect("rendered CJK font must identify its actual asset");
                        assert_eq!(
                            std::path::Path::new(file)
                                .file_name()
                                .and_then(|name| name.to_str()),
                            Some("LXGWWenKaiNerdFont-Regular.ttf"),
                            "rendered CJK font must use the requested LXGW face: {file}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        rendered_han > 0,
        "snapshot must contain a rendered 中 glyph"
    );
}
