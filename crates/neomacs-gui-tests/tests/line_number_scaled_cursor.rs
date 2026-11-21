//! #265: a scaled text cursor must share the text glyphs' gutter origin.
//! GNU proves the geometry invariant; Neo must satisfy it and paint it.
//! GNU pixels are not compared here.
#![cfg(target_os = "linux")]
use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiScenario,
    GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn scaled_cursor_with_line_numbers_and_hl_line_matches_gnu() {
    check_scaled_cursor(GuiBackend::LinuxX11);
}

#[test]
fn wayland_scaled_cursor_with_line_numbers_and_hl_line_matches_gnu() {
    check_scaled_cursor(GuiBackend::LinuxWayland);
}

fn check_scaled_cursor(native_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU oracle");
    eprintln!("{}", oracle.stamp());
    let artifacts = root.join(format!(
        "tmp/cursor265-{}-{}",
        native_backend.slug(),
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let session = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let fontconfig = crate::desktop_font_pins::desktop_font_fontconfig(&artifacts);
    let wayland = (native_backend == GuiBackend::LinuxWayland).then(|| {
        neomacs_infra::display::start_sway(
            &artifacts.join("wayland"),
            "output * resolution 1280x800\nxwayland disable\nseat seat0 fallback true\n",
        )
        .unwrap()
    });
    for scale in [-2, 0, 2, 4] {
        for edge in ["beginning", "end"] {
            for (engine, program) in [("gnu", oracle.executable()), ("neo", binary.as_path())] {
                let directory = artifacts.join(format!("{engine}-{scale}-{edge}"));
                fs::create_dir_all(&directory).unwrap();
                let state_path = directory.join("state.json");
                let backend = if engine == "gnu" {
                    GuiBackend::LinuxX11
                } else {
                    native_backend
                };
                let display_env = if engine == "neo" {
                    wayland
                        .as_ref()
                        .map_or(session.env(), |session| session.env())
                } else {
                    session.env()
                };
                let script =
                    root.join("crates/neomacs-gui-tests/fixtures/line-number-scaled-cursor.el");
                let mut plan = GuiTestPlan::new(
                    backend,
                    &root,
                    &directory,
                    GuiScenario::new("scaled-cursor", &script),
                )
                .with_program(program)
                .with_env("CURSOR265_RESULT", state_path.display().to_string())
                .with_env("CURSOR265_SCALE", scale.to_string())
                .with_env("CURSOR265_EDGE", edge)
                .with_env(
                    "CURSOR265_HIGHLIGHT",
                    if native_backend == GuiBackend::LinuxWayland {
                        "global"
                    } else {
                        "local"
                    },
                )
                .with_env(
                    "GDK_BACKEND",
                    if backend == GuiBackend::LinuxWayland {
                        "wayland"
                    } else {
                        "x11"
                    },
                )
                .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
                .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
                for (key, value) in display_env {
                    plan = plan.with_env(key, value);
                }
                let options = GuiRunOptions::with_timeout(Duration::from_secs(20));
                let readback = GuiArtifactSet::new(&directory, backend, "scaled-cursor").png;
                let png = thread::scope(|scope| {
                    let driver = scope.spawn(|| {
                        let ready = state_path.with_extension("json.ready");
                        let deadline = Instant::now() + Duration::from_secs(15);
                        while !ready.exists() {
                            assert!(
                                Instant::now() < deadline,
                                "fixture not ready: {directory:?}"
                            );
                            thread::sleep(Duration::from_millis(20));
                        }
                        if backend == GuiBackend::LinuxWayland {
                            let runtime = display_env
                                .iter()
                                .find(|(key, _)| key == "XDG_RUNTIME_DIR")
                                .unwrap()
                                .1
                                .clone();
                            let _focused = crate::wayland_readiness::wait_for_focused_child(
                                std::path::Path::new(&runtime),
                                &directory,
                            );
                            let output = neomacs_gui_tests::run_input_tool(
                                {
                                    let mut input = Command::new("wtype");
                                    input.envs(display_env.iter().cloned()).args([
                                        "-d",
                                        "100",
                                        "-k",
                                        "Shift_L",
                                        "-s",
                                        "300",
                                        "-M",
                                        "ctrl",
                                        "-k",
                                        if edge == "end" { "e" } else { "a" },
                                        "-m",
                                        "ctrl",
                                    ]);
                                    input
                                },
                                Duration::from_secs(10),
                            );
                            assert!(
                                output.status.success(),
                                "wtype: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        } else {
                            let pid = fs::read_to_string(ready).unwrap();
                            let input = |args: &[&str]| {
                                let output = Command::new("xdotool")
                                    .args(args)
                                    .envs(display_env.iter().cloned())
                                    .output()
                                    .unwrap();
                                assert!(
                                    output.status.success(),
                                    "xdotool: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                                String::from_utf8(output.stdout).unwrap()
                            };
                            let windows = input(&["search", "--onlyvisible", "--pid", pid.trim()]);
                            let window = windows.lines().last().expect("editor window");
                            input(&["windowfocus", "--sync", window]);
                            input(&[
                                "key",
                                "--clearmodifiers",
                                if edge == "end" { "ctrl+e" } else { "ctrl+a" },
                            ]);
                        }
                        if engine == "neo" {
                            // Preserve the frame at observation time. A later idle
                            // redraw must not erase a transient cursor failure.
                            let deadline = Instant::now() + Duration::from_secs(5);
                            loop {
                                let state_ready = fs::read(&state_path).ok().is_some_and(|bytes| {
                                    serde_json::from_slice::<serde_json::Value>(&bytes).is_ok()
                                });
                                if state_ready
                                    && let Ok(bytes) = fs::read(&readback)
                                    && image::load_from_memory(&bytes).is_ok()
                                {
                                    fs::write(directory.join("observed.png"), bytes).unwrap();
                                    break;
                                }
                                assert!(
                                    Instant::now() < deadline,
                                    "cursor observation/readback missing: {directory:?}"
                                );
                                thread::sleep(Duration::from_millis(10));
                            }
                        }
                    });
                    let png = if engine == "gnu" {
                        let files = plan.write_manifest().unwrap();
                        let output = ProcessGuiCommandRunner
                            .run(&plan.command_spec(), &files, &options)
                            .unwrap();
                        fs::write(&files.stderr, &output.stderr).unwrap();
                        assert_eq!(output.exit_code, Some(0), "{directory:?}");
                        None
                    } else {
                        let output = plan
                            .run_with(&mut ProcessGuiCommandRunner, options)
                            .unwrap();
                        assert_eq!(output.exit_code, Some(0), "{output:#?}");
                        Some(directory.join("observed.png"))
                    };
                    driver.join().unwrap();
                    png
                });
                let state: serde_json::Value =
                    serde_json::from_slice(&fs::read(&state_path).expect("fixture state")).unwrap();
                eprintln!("{engine}/{scale}/{edge}: {state}; {directory:?}");
                assert_eq!(state["family"], "DejaVu Sans Mono");
                assert_eq!(state["point"], if edge == "end" { 9 } else { 1 });
                assert_eq!(state["scale"].as_f64(), Some(f64::from(scale)));
                assert_eq!(state["line_numbers"], true);
                assert_eq!(state["highlight"], true);
                let expected = state[if edge == "end" { "eol" } else { "bol" }]
                    .as_i64()
                    .unwrap();
                let cell_width =
                    (state["eol"].as_i64().unwrap() - state["bol"].as_i64().unwrap()) / 8;
                assert_eq!(
                    state["cursor"][3].as_i64(),
                    Some(cell_width),
                    "cursor width must follow the scaled text at {engine}/{scale}/{edge}: {state}"
                );
                assert_eq!(
                    state["cursor"][1].as_i64(),
                    Some(expected),
                    "{engine}/{scale}/{edge}: {state}"
                );
                if let Some(png) = png {
                    let pixels = image::open(&png).unwrap().to_rgba8();
                    let bounds = |colour: [u8; 3]| {
                        let xs: Vec<u32> = pixels
                            .enumerate_pixels()
                            .filter_map(|(x, _, p)| (p.0[..3] == colour).then_some(x))
                            .collect();
                        (
                            *xs.iter().min().expect("colour present"),
                            *xs.iter().max().unwrap() + 1,
                        )
                    };
                    let reference = bounds([255, 0, 255]);
                    let cursor = bounds([0, 255, 255]);
                    let cell_width = (reference.1 - reference.0) / 8;
                    assert!(
                        (cursor.1 - cursor.0).abs_diff(cell_width) <= 1,
                        "painted cursor width {} != scaled cell {cell_width} at {scale}/{edge}; {directory:?}",
                        cursor.1 - cursor.0
                    );
                    let expected = if edge == "end" {
                        reference.1
                    } else {
                        reference.0
                    };
                    assert!(
                        cursor.0.abs_diff(expected) <= 1,
                        "painted cursor {} != reference {expected} at scale {scale}/{edge}; {directory:?}",
                        cursor.0
                    );
                }
            }
        }
    }
}
