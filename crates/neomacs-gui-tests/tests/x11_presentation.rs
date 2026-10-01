//! Hardware X11 timing validation on an owned editor window. Opt in explicitly:
//! NEOMACS_REQUIRE_X11_TIMING=1 cargo nextest run -p neomacs-gui-tests --test x11_presentation
//! Requires a built editor/pdump, DISPLAY, xdotool and Vulkan EXT_present_timing.
#![cfg(target_os = "linux")]
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

struct Editor(Child);
impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait<T>(artifacts: &Path, reason: &str, mut observe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(value) = observe() {
            return value;
        }
        assert!(Instant::now() < deadline, "{reason}: {artifacts:?}");
        thread::sleep(Duration::from_millis(20));
    }
}
fn state(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
fn samples(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn receipt_number(receipt: &str, key: &str) -> Option<u64> {
    receipt
        .split_once(key)?
        .1
        .split_whitespace()
        .next()?
        .trim_end_matches(')')
        .parse()
        .ok()
}
fn xdotool(args: &[&str]) {
    let result = Command::new("xdotool").args(args).output().unwrap();
    assert!(
        result.status.success(),
        "xdotool: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

struct RestorePointer {
    x: String,
    y: String,
}
impl Drop for RestorePointer {
    fn drop(&mut self) {
        let _ = Command::new("xdotool")
            .args(["mousemove", &self.x, &self.y])
            .status();
    }
}
fn position_pointer_in_owned_window(window: &str) -> RestorePointer {
    let output = Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let location = String::from_utf8(output.stdout).unwrap();
    let field = |prefix| {
        location
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap()
            .to_owned()
    };
    let restore = RestorePointer {
        x: field("X="),
        y: field("Y="),
    };
    xdotool(&["mousemove", "--sync", "--window", window, "150", "150"]);
    restore
}

#[test]
fn x11_rich_scroll_inputs_have_native_output_receipts_across_resize() {
    if std::env::var_os("NEOMACS_REQUIRE_X11_TIMING").is_none() {
        eprintln!("set NEOMACS_REQUIRE_X11_TIMING=1 to require hardware X11 presentation timing");
        return;
    }
    assert!(std::env::var_os("DISPLAY").is_some(), "DISPLAY is required");
    let root = neomacs_infra::workspace_root();
    let parent = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"))
        .join("neomacs-gui-tests");
    let artifacts = parent.join(format!("x11-native-presentation-{}", std::process::id()));
    fs::create_dir_all(&artifacts).unwrap();
    let state_path = artifacts.join("state.json");
    let latency_path = artifacts.join("latency.jsonl");
    let receipt_path = latency_path.with_extension("receipt");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut editor = Editor(
        Command::new(binary)
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures/native-scrolling.el"))
            .env_remove("WAYLAND_DISPLAY")
            .env("WINIT_UNIX_BACKEND", "x11")
            .env_remove("NEOMACS_DEBUG_SURFACE_READBACK")
            .env_remove("NEOMACS_DEBUG_FIRST_FRAME_READBACK")
            .env("NEOMACS_GUI_SCROLL_LINES", "100000")
            .env("NEOMACS_GUI_SCROLL_RICH", "1")
            .env("NEOMACS_GUI_STATE_JSON", &state_path)
            .env("NEOMACS_INPUT_LATENCY_FILE", &latency_path)
            .env(
                "RUST_LOG",
                "warn,neomacs_display_runtime::render_thread::bootstrap=info",
            )
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout")).unwrap())
            .stderr(fs::File::create(artifacts.join("stderr")).unwrap())
            .spawn()
            .unwrap(),
    );
    let initial = wait(&artifacts, "rich fixture did not initialize", || {
        assert!(
            editor.0.try_wait().unwrap().is_none(),
            "editor exited: {artifacts:?}"
        );
        state(&state_path)
    });
    let receipt = wait(&artifacts, "no native X11 output receipt", || {
        let receipt = fs::read_to_string(&receipt_path).ok()?;
        receipt
            .contains(":observation native-first-pixel-output")
            .then_some(receipt)
    });
    assert_eq!(receipt_number(&receipt, ":clock-id "), Some(1));
    assert!(receipt_number(&receipt, ":uncertainty-ns ").unwrap() <= 1_000_000);
    let pid = editor.0.id().to_string();
    let window = wait(&artifacts, "owned X11 window not found", || {
        let result = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--pid", &pid])
            .output()
            .ok()?;
        String::from_utf8(result.stdout)
            .ok()?
            .lines()
            .next()
            .map(str::to_owned)
    });
    // --window targets XSendEvent at this owned window; no global focus change
    // or key injection into whichever application the user happens to use.
    let mut previous = initial;
    const PAGE_INPUTS: usize = 24;
    const WHEEL_INPUTS: usize = 20;
    let page_keys = ["Next", "Next", "Prior", "Prior"]
        .into_iter()
        .chain(["Next", "Prior"].into_iter().cycle().take(PAGE_INPUTS - 4));
    for (index, key) in page_keys.enumerate() {
        if index == 2 {
            let old_receipt = fs::read_to_string(&receipt_path).unwrap();
            xdotool(&["windowsize", &window, "860", "640"]);
            wait(
                &artifacts,
                "no native receipt after swapchain resize",
                || {
                    let new = fs::read_to_string(&receipt_path).ok()?;
                    (receipt_number(&new, ":submission ")
                        > receipt_number(&old_receipt, ":submission ")
                        && receipt_number(&new, ":width ")
                            != receipt_number(&old_receipt, ":width "))
                    .then_some(())
                },
            );
        }
        xdotool(&["key", "--window", &window, key]);
        let after = wait(&artifacts, "page key did not change the viewport", || {
            let current = state(&state_path)?;
            (current["processed-pages"].as_u64()? >= index as u64 + 1
                && current["start"] != previous["start"])
                .then_some(current)
        });
        previous = after;
        let evidence = wait(
            &artifacts,
            "page input has no native timing evidence",
            || {
                let all = samples(&latency_path);
                (all.len() >= index + 1).then_some(all)
            },
        );
        for sample in evidence {
            assert_eq!(sample["kind"], "page");
            assert_eq!(sample["observation"], "native-first-pixel-output");
            assert!(sample["timestamp_uncertainty_ns"].as_u64().unwrap() <= 1_000_000);
            assert!(
                sample["input_to_present_ns"].as_u64().is_some(),
                "invalid clock: {sample}"
            );
        }
    }
    // X11 wheel input uses XI2/XTest; core XSendEvent button events do not
    // reach winit's XI2 wheel path. Restore the pointer even if an assertion fails.
    let _restore_pointer = position_pointer_in_owned_window(&window);
    for (index, button) in ["5", "4"]
        .into_iter()
        .cycle()
        .take(WHEEL_INPUTS)
        .enumerate()
    {
        let before = wait(&artifacts, "no complete state before wheel input", || {
            state(&state_path)
        });
        xdotool(&["click", button]);
        wait(&artifacts, "wheel did not change the viewport", || {
            let current = state(&state_path)?;
            (current["processed-wheels"].as_u64()? > before["processed-wheels"].as_u64()?
                && current["start"] != before["start"])
                .then_some(())
        });
        let evidence = wait(
            &artifacts,
            "wheel input has no native timing evidence",
            || {
                let all = samples(&latency_path);
                (all.len() >= PAGE_INPUTS + index + 1).then_some(all)
            },
        );
        let sample = evidence.last().unwrap();
        assert_eq!(sample["kind"], "wheel");
        assert_eq!(sample["observation"], "native-first-pixel-output");
        assert!(sample["input_to_present_ns"].as_u64().is_some());
    }
    let evidence = samples(&latency_path);
    assert_eq!(evidence.len(), PAGE_INPUTS + WHEEL_INPUTS);
    // Keep the first startup/font-warming input distinct. Resize and all later
    // page/wheel commands remain in the warm performance gate.
    let mut summary = serde_json::Map::new();
    summary.insert(
        "cold_page_ns".into(),
        evidence[0]["input_to_present_ns"].clone(),
    );
    for kind in ["page", "wheel"] {
        let mut times: Vec<u64> = evidence
            .iter()
            .skip(1)
            .filter(|sample| sample["kind"] == kind)
            .map(|sample| {
                sample["input_to_present_ns"]
                    .as_u64()
                    .expect("valid native clock")
            })
            .collect();
        times.sort_unstable();
        let p95 = times[(times.len() * 95).div_ceil(100) - 1];
        let maximum = *times.last().unwrap();
        summary.insert(
            kind.into(),
            serde_json::json!({"count": times.len(), "p95_ns": p95, "max_ns": maximum}),
        );
    }
    fs::write(
        artifacts.join("latency-summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    // Inject the first page as soon as fixture construction completes, without
    // waiting for its first redisplay or warming its mixed-font fallbacks.
    // This catches whole-font-collection copies delaying queued input.
    assert!(
        summary["cold_page_ns"].as_u64().unwrap() <= 250_000_000,
        "cold page latency exceeds 250 ms: {}; artifacts: {}",
        summary["cold_page_ns"],
        artifacts.display()
    );
    for kind in ["page", "wheel"] {
        let stats = &summary[kind];
        assert!(
            stats["p95_ns"].as_u64().unwrap() <= 50_000_000
                && stats["max_ns"].as_u64().unwrap() <= 250_000_000,
            "warm {kind} latency exceeds p95 50 ms / max 250 ms: {stats}; artifacts: {}",
            artifacts.display()
        );
    }
    fs::write(
        artifacts.join("validated"),
        "native X11 rich page/wheel input and resize receipts\n",
    )
    .unwrap();
}
