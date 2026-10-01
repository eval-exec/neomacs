//! Native precise scroll diagnostic on an isolated headless Wayland compositor.
//! This exercises the shared PixelDelta path, not AppKit's event generation.
#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};
#[path = "native_scrolling/continuous.rs"]
mod continuous;
#[path = "native_scrolling/wayland.rs"]
mod wayland;

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state(path: &Path, after: u64) -> Value {
    state_with_timeout(path, after, Duration::from_secs(8))
}

fn state_with_timeout(path: &Path, after: u64, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && value["sample"]
                .as_u64()
                .is_some_and(|sample| sample > after)
        {
            return value;
        }
        assert!(Instant::now() < deadline, "no fresh state: {path:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn state_after_input(path: &Path, after: u64, counter: &str, expected: f64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut sample = after;
    loop {
        let value = state(path, sample);
        if value[counter].as_f64().unwrap() >= expected {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "scroll input not completed: {value}"
        );
        sample = value["sample"].as_u64().unwrap();
    }
}

fn readback(path: &Path) -> image::DynamicImage {
    // The running renderer rewrites its diagnostic PNG on each frame. A
    // timer sample does not synchronize file publication: wait for a complete
    // PNG instead of treating a concurrent write as a rendering failure.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match image::open(path) {
            Ok(image) => return image,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "no complete GUI readback at {path:?}: {error}"
                );
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
// Prerequisites: fresh release Neomacs/runtime and sway on PATH.
fn precise_native_scroll_advances_without_snapback() {
    run_native_scroll(ScrollKind::Precise, ScrollTarget::Selected);
}

#[test]
fn native_wheel_scroll_advances_without_snapback() {
    run_native_scroll(ScrollKind::Wheel, ScrollTarget::Selected);
}

#[test]
fn precise_native_scroll_targets_the_unselected_window_under_the_pointer() {
    run_native_scroll(ScrollKind::Precise, ScrollTarget::OtherWindow);
}

#[test]
fn precise_native_scroll_bursts_advance_without_snapback() {
    run_native_scroll(ScrollKind::PreciseBurst, ScrollTarget::Selected);
}

#[test]
fn native_page_keys_advance_and_have_confirmed_input_latency() {
    run_native_scroll(ScrollKind::Page, ScrollTarget::Selected);
}

#[test]
fn precise_native_bursts_in_a_large_buffer_return_to_the_initial_viewport() {
    run_native_scroll_in_buffer(ScrollKind::PreciseBurst, ScrollTarget::Selected, 100_000);
}

#[test]
fn native_page_keys_in_a_large_buffer_have_confirmed_input_latency() {
    run_native_scroll_in_buffer(ScrollKind::Page, ScrollTarget::Selected, 100_000);
}

#[test]
fn rich_large_buffer_native_page_keys_have_confirmed_input_latency() {
    run_native_scroll_profile(ScrollKind::Page, ScrollTarget::Selected, 100_000, true);
}

#[test]
fn rich_large_buffer_native_precise_bursts_return_to_the_initial_viewport() {
    run_native_scroll_profile(
        ScrollKind::PreciseBurst,
        ScrollTarget::Selected,
        100_000,
        true,
    );
}

#[test]
fn rich_large_buffer_native_wheel_scroll_advances() {
    run_native_scroll_profile(ScrollKind::Wheel, ScrollTarget::Selected, 100_000, true);
}

#[test]
fn continuous_120hz_scroll_in_large_plain_buffer() {
    run_native_scroll_profile(
        ScrollKind::PreciseStream,
        ScrollTarget::Selected,
        100_000,
        false,
    );
}

#[test]
fn continuous_120hz_scroll_in_large_rich_buffer() {
    run_native_scroll_profile(
        ScrollKind::PreciseStream,
        ScrollTarget::Selected,
        100_000,
        true,
    );
}

#[derive(Clone, Copy, Debug)]
enum ScrollKind {
    Precise,
    PreciseBurst,
    PreciseStream,
    Wheel,
    Page,
}

#[derive(Clone, Copy, Debug)]
enum ScrollTarget {
    Selected,
    OtherWindow,
}

fn run_native_scroll(kind: ScrollKind, target: ScrollTarget) {
    run_native_scroll_in_buffer(kind, target, 400);
}

fn run_native_scroll_in_buffer(kind: ScrollKind, target: ScrollTarget, lines: usize) {
    run_native_scroll_profile(kind, target, lines, false);
}

fn run_native_scroll_profile(kind: ScrollKind, target: ScrollTarget, lines: usize, rich: bool) {
    run_native_scroll_scenario(kind, target, lines, rich, false, false);
}

#[test]
fn precise_scroll_presents_pixels_while_evaluator_is_stalled() {
    run_native_scroll_scenario(
        ScrollKind::Precise,
        ScrollTarget::Selected,
        400,
        false,
        true,
        false,
    );
}

#[test]
fn page_scroll_presents_resolved_destination_while_command_is_stalled() {
    run_native_scroll_scenario(
        ScrollKind::Page,
        ScrollTarget::Selected,
        400,
        false,
        true,
        true,
    );
}

#[test]
fn wheel_scroll_presents_resolved_destination_while_command_is_stalled() {
    run_native_scroll_scenario(
        ScrollKind::Wheel,
        ScrollTarget::Selected,
        400,
        false,
        true,
        true,
    );
}

#[test]
fn rich_large_buffer_page_presents_while_command_is_stalled() {
    run_native_scroll_scenario(
        ScrollKind::Page,
        ScrollTarget::Selected,
        100_000,
        true,
        true,
        true,
    );
}

#[test]
fn rich_large_buffer_wheel_presents_while_command_is_stalled() {
    run_native_scroll_scenario(
        ScrollKind::Wheel,
        ScrollTarget::Selected,
        100_000,
        true,
        true,
        true,
    );
}

fn run_native_scroll_scenario(
    kind: ScrollKind,
    target: ScrollTarget,
    lines: usize,
    rich: bool,
    stalled: bool,
    resolved: bool,
) {
    let profile = if rich { "rich-v1" } else { "plain" };
    let root = neomacs_infra::workspace_root();
    let artifact_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(|path| root.join(PathBuf::from(path)))
        .unwrap_or_else(|| root.join("target"))
        .join("neomacs-gui-tests");
    fs::create_dir_all(&artifact_root).unwrap();
    let artifacts = artifact_root.join(format!(
        "native-scrolling-{kind:?}-{target:?}-{lines}-{profile}-{}",
        std::process::id()
    ));
    fs::create_dir(&artifacts).unwrap();
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        r#"
output * resolution 1000x700
xwayland disable
seat seat0 fallback true
default_border none
focus_follows_mouse yes
"#,
    )
    .expect("start sway");
    let session_env: std::collections::HashMap<String, String> =
        session.env().iter().cloned().collect();
    let runtime = session_env["XDG_RUNTIME_DIR"].clone();
    let socket = session_env["WAYLAND_DISPLAY"].clone();
    let state_path = artifacts.join("state.json");
    let pixels_path = artifacts.join("surface.png");
    let latency_path = artifacts.join("input-latency.jsonl");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    // Run the default pixel correctness pass before a timing-only pass.
    // Continuous GPU readback/PNG encoding would dominate input latency.
    // Sustained streams measure throughput; readback and per-glyph debug
    // logging would change the workload. Burst tests cover rendered pixels.
    let timing_only = matches!(kind, ScrollKind::PreciseStream)
        || std::env::var_os("NEOMACS_GUI_SCROLL_TIMING_ONLY").is_some();
    fs::write(
        artifacts.join("measurement-mode"),
        if timing_only {
            "timing-only\n"
        } else {
            "pixel-correctness\n"
        },
    )
    .unwrap();
    let mut command = Command::new(binary);
    let stall_path = artifacts.join("evaluator-stall");
    command.env("NEOMACS_GUI_SCROLL_STALL", &stall_path);
    command.env_remove("NEOMACS_GUI_SCROLL_RESOLVED_STALL");
    if resolved {
        command.env("NEOMACS_GUI_SCROLL_RESOLVED_STALL", "1");
    }
    command
        .env_remove("NEOMACS_DEBUG_SURFACE_READBACK")
        .env_remove("NEOMACS_DEBUG_SURFACE_READBACK_PNG")
        .env_remove("WAYLAND_DEBUG")
        .env_remove("NEOMACS_LAYOUT_STATS_FILE");
    if !timing_only {
        command
            .env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
            .env("NEOMACS_DEBUG_SURFACE_READBACK_PNG", &pixels_path)
            .env("WAYLAND_DEBUG", "1")
            .env(
                "NEOMACS_LAYOUT_STATS_FILE",
                artifacts.join("layout-stats.log"),
            );
    }
    command.env_remove("NEOMACS_GUI_SCROLL_RICH");
    if rich {
        command.env("NEOMACS_GUI_SCROLL_RICH", "1");
    }
    command.env_remove("NEOMACS_GUI_SCROLL_OTHER_WINDOW");
    if matches!(target, ScrollTarget::OtherWindow) {
        command.env("NEOMACS_GUI_SCROLL_OTHER_WINDOW", "1");
    }
    let mut editor = OwnedChild(
        command
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures/native-scrolling.el"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WAYLAND_DISPLAY", &socket)
            .env("WINIT_UNIX_BACKEND", "wayland")
            .env_remove("DISPLAY")
            .env("NEOMACS_GUI_SCROLL_LINES", lines.to_string())
            .env("NEOMACS_GUI_STATE_JSON", &state_path)
            .env("NEOMACS_INPUT_LATENCY_FILE", &latency_path)
            .env(
                "RUST_LOG",
                std::env::var("NEOMACS_GUI_SCROLL_LOG").unwrap_or_else(|_| {
                    if timing_only {
                        // One startup record identifies the actual adapter;
                        // keep per-frame/glyph logging disabled for timing.
                        "warn,neomacs_display_runtime::render_thread::bootstrap=info"
                    } else {
                        "warn,neomacs=debug,neomacs_display_runtime=debug,neomacs_layout_engine::scroll_coverage=debug,neovm_core::scroll_prediction=debug"
                    }.to_owned()
                }),
            )
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout")).unwrap())
            .stderr(fs::File::create(artifacts.join("stderr")).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut trackpad = wayland::Trackpad::connect(&PathBuf::from(&runtime).join(&socket));
    let initial = state_with_timeout(&state_path, 2, Duration::from_secs(90));
    if rich {
        assert_eq!(initial["content"]["profile"], "rich-v1");
        assert_eq!(initial["content"]["lines"].as_u64(), Some(lines as u64));
        assert_eq!(initial["content"]["face-variants"], 6);
        assert_eq!(initial["content"]["font-selection"], "installed");
        assert!(
            initial["content"]["font-families"]
                .as_array()
                .unwrap()
                .len()
                >= 3
        );
        assert_eq!(
            initial["content"]["overlays"].as_u64(),
            Some((lines.div_ceil(32) * 4) as u64)
        );
        assert!(initial["buffer-size"].as_u64().unwrap() > (lines * 40) as u64);
    } else {
        assert_eq!(initial["buffer-size"].as_u64(), Some((lines * 40) as u64));
    }
    if lines > 400 {
        assert!(
            initial["start"].as_u64().unwrap() > 1_000_000,
            "large-buffer scrolling must exercise a viewport far from buffer start"
        );
    }
    trackpad.move_to_body();
    thread::sleep(Duration::from_millis(200));
    let mut previous = state(&state_path, initial["sample"].as_u64().unwrap());
    let initial_pixels = (!timing_only).then(|| readback(&pixels_path));
    if let Some(pixels) = &initial_pixels {
        pixels.save(artifacts.join("before.png")).unwrap();
    }
    // Sample text away from point at the left edge: moving the cursor alone
    // must not make a stale text presentation look like successful scrolling.
    let text_pixels = |pixels: &image::DynamicImage| {
        pixels
            .crop_imm(
                pixels.width() / 25,
                pixels.height() / 8,
                pixels.width() / 2,
                pixels.height() / 2,
            )
            .to_rgba8()
    };
    let initial_text = initial_pixels.as_ref().map(text_pixels);
    let position = |s: &Value| (s["start"].as_i64().unwrap(), s["vscroll"].as_i64().unwrap());
    if stalled {
        assert!(
            !timing_only,
            "stall proof requires pixel and presentation evidence"
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while !fs::read_to_string(artifacts.join("neomacs.log"))
            .unwrap_or_default()
            .contains(if resolved {
                "exported compositor coverage"
            } else {
                "predict_pixels=true"
            })
        {
            assert!(
                Instant::now() < deadline,
                "no certified prediction policy: {artifacts:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let before = text_pixels(&readback(&pixels_path));
        let sent = Instant::now();
        fs::write(&stall_path, b"hold").unwrap();
        if resolved {
            match kind {
                ScrollKind::Wheel => trackpad.wheel(),
                ScrollKind::Page => {
                    let status = Command::new("wtype")
                        .env("XDG_RUNTIME_DIR", &runtime)
                        .env("WAYLAND_DISPLAY", &socket)
                        .args(["-s", "200", "-k", "Next", "-s", "100"])
                        .status()
                        .unwrap();
                    assert!(status.success());
                }
                _ => unreachable!(),
            }
        }
        while !stall_path.with_extension("stalled").exists() {
            assert!(
                Instant::now() < deadline,
                "evaluator did not enter stall: {artifacts:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let frozen = fs::read(&state_path).unwrap();
        if resolved {
            let destination: Value =
                serde_json::from_slice(&fs::read(stall_path.with_extension("stalled")).unwrap())
                    .unwrap();
            assert!(destination["start"].as_i64().unwrap() > previous["start"].as_i64().unwrap());
        }
        let projected_submission = || {
            let log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap_or_default();
            let mut requested = None;
            for line in log.lines() {
                if line.contains("requested native presentation feedback") {
                    requested = line
                        .split_once("serial: ")
                        .and_then(|(_, tail)| tail.split(',').next())
                        .and_then(|number| number.parse::<u64>().ok());
                }
                if line.contains("submitted input-driven scroll") {
                    return requested;
                }
            }
            None
        };
        let confirmed_submission = || {
            let receipt =
                fs::read_to_string(latency_path.with_extension("receipt")).unwrap_or_default();
            receipt
                .split_once(":submission ")
                .and_then(|(_, tail)| tail.split_whitespace().next())
                .and_then(|number| number.parse::<u64>().ok())
        };
        if !resolved {
            trackpad.scroll(4.0);
        }
        loop {
            let changed = text_pixels(&readback(&pixels_path)) != before;
            let confirmed = projected_submission()
                .zip(confirmed_submission())
                .is_some_and(|(projected, confirmed)| confirmed >= projected);
            if changed && confirmed {
                break;
            }
            assert!(
                sent.elapsed() < Duration::from_secs(4),
                "no projected pixels with native presentation while evaluator is stalled: {artifacts:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        if !resolved {
            assert!(
                fs::read_to_string(artifacts.join("neomacs.log"))
                    .unwrap()
                    .contains("composited retained scroll raster"),
                "stalled precision scroll must exercise the pooled body raster: {artifacts:?}"
            );
        }
        assert_eq!(
            fs::read(&state_path).unwrap(),
            frozen,
            "evaluator advanced before the projected presentation"
        );
        readback(&pixels_path)
            .save(artifacts.join("projected-while-stalled.png"))
            .unwrap();
        fs::write(
            artifacts.join("stalled-projection-proof.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "observed_projection_ms": sent.elapsed().as_secs_f64() * 1000.0,
                "confirmed_native_feedback": true,
            "projected_submission": projected_submission(),
            "confirmed_submission": confirmed_submission(),
                "evaluator_state_unchanged": true
            }))
            .unwrap(),
        )
        .unwrap();
        fs::remove_file(&stall_path).unwrap();
        let (counter, count) = match kind {
            ScrollKind::Page => ("processed-pages", 1.0),
            ScrollKind::Wheel => ("processed-wheels", 1.0),
            _ => ("processed-pixels", 4.0),
        };
        let completed = state_after_input(
            &state_path,
            previous["sample"].as_u64().unwrap(),
            counter,
            previous[counter].as_f64().unwrap() + count,
        );
        assert!(position(&completed) > position(&previous));
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let samples: Vec<Value> = fs::read_to_string(&latency_path)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            if let Some(sample) = samples
                .iter()
                .find(|sample| sample["input_to_projected_present_ns"].is_u64())
            {
                assert_eq!(
                    sample["projected_submission"].as_u64(),
                    projected_submission()
                );
                assert!(
                    sample["input_to_projected_present_ns"].as_u64().unwrap()
                        < sample["input_to_present_ns"].as_u64().unwrap()
                );
                fs::write(
                    artifacts.join("stalled-latency-proof.json"),
                    serde_json::to_vec_pretty(sample).unwrap(),
                )
                .unwrap();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no exact-submission projected latency: {artifacts:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        return;
    }

    if matches!(kind, ScrollKind::PreciseStream) {
        continuous::run(
            &mut trackpad,
            &artifacts,
            &state_path,
            &latency_path,
            previous,
        );
        return;
    }

    let initial_position = position(&previous);
    let mut processed_pixels = previous["processed-pixels"].as_f64().unwrap();
    let mut processed_wheels = previous["processed-wheels"].as_f64().unwrap();
    let mut processed_pages = previous["processed-pages"].as_f64().unwrap();
    let mut trace = vec![previous.clone()];
    let steps = match kind {
        ScrollKind::Precise | ScrollKind::PreciseBurst | ScrollKind::PreciseStream => 24,
        ScrollKind::Wheel => 12,
        ScrollKind::Page => 8,
    };
    for step in 0..steps {
        let down = step
            < if matches!(kind, ScrollKind::Page) {
                4
            } else {
                12
            };
        match kind {
            ScrollKind::Precise => trackpad.scroll(if down { 4.0 } else { -4.0 }),
            ScrollKind::PreciseBurst | ScrollKind::PreciseStream => {
                // Deliver a burst without waiting for a redisplay between
                // events. Observe direction and rendered text after each
                // batch, then reverse while preserving the same device.
                for _ in 0..8 {
                    trackpad.scroll(if down { 4.0 } else { -4.0 });
                }
            }
            ScrollKind::Wheel => trackpad.wheel(),
            ScrollKind::Page => {
                let status = Command::new("wtype")
                    .env("XDG_RUNTIME_DIR", &runtime)
                    .env("WAYLAND_DISPLAY", &socket)
                    .args([
                        "-s",
                        "200",
                        "-k",
                        if down { "Next" } else { "Prior" },
                        "-s",
                        "100",
                    ])
                    .status()
                    .expect("wtype is required for native page-key testing");
                assert!(status.success());
            }
        }
        thread::sleep(Duration::from_millis(500));
        let after = previous["sample"].as_u64().unwrap();
        // A timer sample can precede this batch even after the sleep. Wait
        // for command completion, so delayed input is not mistaken for
        // snapback. This is a correctness test, not a latency measurement.
        let current = if matches!(
            kind,
            ScrollKind::Precise | ScrollKind::PreciseBurst | ScrollKind::PreciseStream
        ) {
            processed_pixels += if matches!(kind, ScrollKind::PreciseBurst) {
                32.0
            } else {
                4.0
            };
            state_after_input(&state_path, after, "processed-pixels", processed_pixels)
        } else if matches!(kind, ScrollKind::Page) {
            processed_pages += 1.0;
            state_after_input(&state_path, after, "processed-pages", processed_pages)
        } else {
            processed_wheels += 1.0;
            state_after_input(&state_path, after, "processed-wheels", processed_wheels)
        };
        trace.push(current.clone());
        fs::write(
            artifacts.join("trace.json"),
            serde_json::to_vec_pretty(&trace).unwrap(),
        )
        .unwrap();
        let pixels = (!timing_only).then(|| readback(&pixels_path));
        if let Some(pixels) = &pixels {
            pixels
                .save(artifacts.join(format!("step-{step}.png")))
                .unwrap();
        }
        eprintln!("step={step} before={previous} after={current}; artifacts={artifacts:?}");
        if !timing_only
            && !rich
            && matches!(target, ScrollTarget::Selected)
            && matches!(
                kind,
                ScrollKind::Page | ScrollKind::Wheel | ScrollKind::Precise
            )
            && step == 3
        {
            // These four forward viewports have never been displayed. Historical page
            // replay cannot satisfy this check: idle worker coverage must
            // actually reach the native command's accepted presentation.
            let stats = fs::read_to_string(artifacts.join("layout-stats.log")).unwrap();
            assert!(
                stats.split_whitespace().any(|field| field
                    .strip_prefix("prepared=")
                    .and_then(|value| value.parse::<usize>().ok())
                    .is_some_and(|count| count > 0)),
                "native forward scrolling never used its precomputed rows; artifacts={artifacts:?}"
            );
        }
        assert!(editor.0.try_wait().unwrap().is_none(), "editor exited");
        assert_eq!(
            current["selected"], initial["selected"],
            "scrolling must preserve window selection"
        );
        if matches!(target, ScrollTarget::OtherWindow) {
            assert_eq!(
                current["selected-start"], initial["selected-start"],
                "scrolling the other window must not move the selected viewport"
            );
        }
        if !timing_only {
            let native_log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap();
            let expected_event = match kind {
                ScrollKind::Precise | ScrollKind::PreciseBurst | ScrollKind::PreciseStream => {
                    "PixelScroll {"
                }
                ScrollKind::Wheel => "MouseScroll {",
                ScrollKind::Page => "KeyPress {",
            };
            assert!(
                native_log.contains(expected_event),
                "native input did not reach the VM bridge as {expected_event}: {artifacts:?}"
            );
        }
        assert!(
            if down {
                position(&current) > position(&previous)
            } else {
                position(&current) < position(&previous)
            },
            "{kind:?} scrolling must follow the gesture without snapping back: down={down}, before={previous}, after={current}; artifacts={artifacts:?}"
        );
        if step == 0 && matches!(kind, ScrollKind::Precise) {
            assert_eq!(
                position(&current),
                (initial_position.0, initial_position.1 + 4),
                "a four-pixel gesture must not fall back to whole-line wheel scrolling"
            );
        }
        previous = current;
        if (step == 11 || (step == 3 && matches!(kind, ScrollKind::Page))) && !timing_only {
            assert!(
                Some(text_pixels(pixels.as_ref().unwrap())) != initial_text,
                "scrolling must update rendered text, not only Lisp state: {artifacts:?}"
            );
        }
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    let samples = loop {
        let samples: Vec<Value> = fs::read_to_string(&latency_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let expected = steps
            * if matches!(kind, ScrollKind::PreciseBurst) {
                8
            } else {
                1
            };
        if samples.len() >= expected {
            assert_eq!(samples.len(), expected);
            break samples;
        }
        assert!(
            Instant::now() < deadline,
            "missing causal presentation receipts: {samples:?}; {artifacts:?}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let expected_kind = match kind {
        ScrollKind::Wheel => "wheel",
        ScrollKind::Page => "page",
        _ => "precise",
    };
    for sample in &samples {
        assert_eq!(sample["kind"], expected_kind);
        assert!(
            sample["input_to_present_ns"].as_u64().is_some(),
            "clock mismatch: {sample}"
        );
        assert!(sample["presentation"].as_u64().unwrap() > 0);
        assert_eq!(sample["evicted_inputs"], 0);
    }
    if matches!(
        kind,
        ScrollKind::Precise | ScrollKind::PreciseBurst | ScrollKind::PreciseStream
    ) {
        assert_eq!(
            position(&previous),
            initial_position,
            "opposite precise gestures must return to the initial viewport"
        );
    }
}
