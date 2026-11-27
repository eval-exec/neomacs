//! #857 Wayland GUI acceptance row: present loop goes idle after visual effects
//! are disabled mid-animation.
//!
//! GPU-only row (`gpu_only_`; excluded on software-only CI via
//! `not test(~gpu_only_)`, run unconditionally by GPU-backed GUI-PARALLEL):
//! `QualityMode::SoftwareCompatibility` disables dynamic effects and cursor
//! effects by policy, so testing that disabling `cursor-comet` + `cursor-motion`
//! mid-animation drains renderer effect state and lets the present loop go idle
//! requires `QualityMode::Full` under GL Weston (`WestonBenchSession::start`).

#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use neomacs_gui_tests::{HangCap, NegativeSoak, unique_nonce};
use neomacs_infra::display::{WestonBenchConfig, WestonBenchSession, WestonDesktop};
use serde_json::Value;

struct KillOnDrop(Option<Child>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresentSample {
    presents: u64,
    cursor_effect: u64,
    redisplay: u64,
}

fn free_loopback_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral loopback port")
        .local_addr()
        .expect("read ephemeral port")
        .port()
}

fn fetch_metrics(port: u16) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect diagnostics server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream
        .write_all(b"GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("write GET /metrics");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read /metrics response");
    let (_, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("malformed HTTP response: {response}"));
    serde_json::from_str(body).unwrap_or_else(|err| panic!("parse /metrics JSON: {err}: {body}"))
}

fn metric_u64(metrics: &Value, key: &str) -> u64 {
    metrics["frame"][key]
        .as_u64()
        .unwrap_or_else(|| panic!("missing numeric metric frame.{key} in {metrics}"))
}

fn demand_reason_u64(metrics: &Value, reason: &str) -> u64 {
    metrics["frame"]["demand_reasons"][reason]
        .as_u64()
        .unwrap_or_else(|| {
            panic!("missing numeric metric frame.demand_reasons.{reason} in {metrics}")
        })
}

fn sample_present_metrics(port: u16) -> PresentSample {
    let m = fetch_metrics(port);
    PresentSample {
        presents: metric_u64(&m, "presents"),
        cursor_effect: demand_reason_u64(&m, "cursor_effect"),
        redisplay: demand_reason_u64(&m, "redisplay"),
    }
}

fn wait_for_file(path: &Path, child: &mut Child, timeout: Duration, label: &str) {
    let cap = HangCap::new(timeout);
    loop {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll editor process") {
            panic!("editor exited early ({status}) while waiting for {label} ({path:?})");
        }
        cap.assert_not_expired(format!("waiting for {label} ({path:?})"));
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
// Prerequisites: requires release binary/pdump, GL Weston (--renderer=gl), and a hardware GPU adapter (quality_mode=Full).
fn wayland_gpu_only_disabled_effects_present_loop_goes_idle() {
    let root = neomacs_infra::workspace_root();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    assert!(
        binary.exists(),
        "build {binary:?} before running disabled-effects idle test"
    );

    let nonce = unique_nonce();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/disabled-effects-idle-{nonce}"
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let receipt_path = artifacts.join("presentation.sexp");
    let stdout_path = artifacts.join("stdout.log");
    let stderr_path = artifacts.join("stderr.log");
    let port = free_loopback_port();

    let session = WestonBenchSession::start(
        &artifacts.join("display"),
        WestonBenchConfig {
            width: 1280,
            height: 800,
            xwayland: false,
            desktop: WestonDesktop::Solid,
        },
    )
    .unwrap_or_else(|err| {
        panic!("GPU-only disabled-effects idle row requires GL Weston (--renderer=gl): {err}")
    });

    let fixture = root.join("crates/neomacs-gui-tests/fixtures/disabled-effects-idle.el");
    let mut cmd = Command::new(&binary);
    cmd.args([
        "-Q",
        "--font=DejaVu Sans Mono-13",
        "--geometry=100x35",
        "-l",
        fixture.to_str().unwrap(),
    ])
    .env_remove("DISPLAY")
    .env("NEOMACS_RUNTIME_ROOT", &root)
    .env("NEOMACS_DIAGNOSTICS_PORT", port.to_string())
    .env("NEOMACS_IDLE_TEST_DIR", &artifacts)
    .env("NEOMACS_GUI_PRESENTATION_RECEIPT", &receipt_path)
    .env(
        "RUST_LOG",
        "error,neomacs_display_runtime::render_thread::state=info,neomacs_display_runtime::render_thread::gpu_startup=info",
    )
    .stdin(Stdio::null())
    .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
    .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()));
    for key in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
    for (key, value) in session.env() {
        cmd.env(key, value);
    }

    let mut guard = KillOnDrop(Some(cmd.spawn().expect("spawn neomacs")));
    let child = guard.0.as_mut().expect("editor active");

    wait_for_file(
        &artifacts.join("ready"),
        child,
        Duration::from_secs(30),
        "ready",
    );
    let baseline = sample_present_metrics(port);

    // 1. Enable cursor-comet (long fade) + cursor-motion and move point so the
    //    comet trail fills and drives real `cursor_effect` frame demand.
    fs::write(artifacts.join("enable-go"), "go\n").unwrap();
    wait_for_file(
        &artifacts.join("effects-active"),
        child,
        Duration::from_secs(30),
        "effects-active",
    );

    let active_cap = HangCap::new(Duration::from_secs(10));
    let active_sample = loop {
        let s = sample_present_metrics(port);
        if s.cursor_effect > baseline.cursor_effect {
            break s;
        }
        if let Some(status) = child.try_wait().expect("poll editor process") {
            panic!("editor exited early ({status}) while waiting for cursor_effect demand");
        }
        active_cap.assert_not_expired(format!(
            "enabled cursor-comet never produced cursor_effect demand (baseline={baseline:?}, latest={s:?})"
        ));
        thread::sleep(Duration::from_millis(50));
    };

    // 2. Disable every enabled effect mid-animation while the 5s comet trail is
    //    still non-empty.
    fs::write(artifacts.join("disable-go"), "go\n").unwrap();
    wait_for_file(
        &artifacts.join("disabled"),
        child,
        Duration::from_secs(30),
        "disabled",
    );

    // 3. Bounded settle: poll /metrics every 100 ms up to a HangCap, requiring
    //    N consecutive identical (presents, cursor_effect, redisplay) samples.
    //    Under #857 (frozen disabled effect state), `presents` and
    //    `cursor_effect` keep climbing on every sample until the cap expires.
    const REQUIRED_IDENTICAL_SAMPLES: usize = 6;
    let settle_cap = HangCap::new(Duration::from_secs(10));
    let mut series: Vec<PresentSample> = Vec::new();
    let mut consecutive_identical = 0usize;
    let settled = loop {
        let s = sample_present_metrics(port);
        if series.last() == Some(&s) {
            consecutive_identical += 1;
        } else {
            consecutive_identical = 1;
        }
        series.push(s);
        if consecutive_identical >= REQUIRED_IDENTICAL_SAMPLES {
            break s;
        }
        if let Some(status) = child.try_wait().expect("poll editor process") {
            panic!(
                "editor exited early ({status}) while settling after disable; series={series:?}"
            );
        }
        let first = series[0];
        settle_cap.assert_not_expired(format!(
            "present loop did not go idle after disabling effects (need {REQUIRED_IDENTICAL_SAMPLES} consecutive identical 100ms samples; delta since disable start: presents=+{}, cursor_effect=+{}, redisplay=+{}; series={series:?})",
            s.presents.saturating_sub(first.presents),
            s.cursor_effect.saturating_sub(first.cursor_effect),
            s.redisplay.saturating_sub(first.redisplay),
        ));
        thread::sleep(Duration::from_millis(100));
    };

    // 4. Negative soak: hold for >= 1s after settle and verify zero new
    //    presents, zero cursor_effect demands, and zero redisplay demands.
    let soak = NegativeSoak::observe(Duration::from_secs(1), Duration::from_millis(100), || {
        sample_present_metrics(port)
    });
    let after_soak = *soak.last_receipt();

    // Clean shutdown and quality_mode=Full check.
    fs::write(artifacts.join("quit"), "quit\n").unwrap();
    let exit_cap = HangCap::new(Duration::from_secs(15));
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for clean exit") {
            break status;
        }
        exit_cap.assert_not_expired("editor did not exit cleanly");
        thread::sleep(Duration::from_millis(20));
    };
    let _ = guard.0.take();
    assert!(
        status.success(),
        "editor exited with {status}; stderr={}",
        fs::read_to_string(&stderr_path).unwrap_or_default()
    );

    let stdout_raw = fs::read_to_string(&stdout_path).unwrap_or_default();
    let stdout = neomacs_gui_tests::interaction::strip_ansi(&stdout_raw);
    let final_policy_line = stdout
        .lines()
        .rfind(|line| line.contains("applying render-quality policy"))
        .unwrap_or("<quality_mode line absent>");
    assert!(
        final_policy_line.contains("quality_mode=Full"),
        "GPU-only disabled-effects idle row requires quality_mode=Full (refusing software fallback): {final_policy_line}"
    );

    let soak_presents_delta = after_soak.presents.saturating_sub(settled.presents);
    let soak_cursor_effect_delta = after_soak
        .cursor_effect
        .saturating_sub(settled.cursor_effect);
    let soak_redisplay_delta = after_soak.redisplay.saturating_sub(settled.redisplay);

    eprintln!(
        "disabled-effects-idle summary: policy={final_policy_line} baseline={baseline:?} active={active_sample:?} settled={settled:?} after_soak={after_soak:?} settle_samples={} soak_delta(presents={soak_presents_delta} cursor_effect={soak_cursor_effect_delta} redisplay={soak_redisplay_delta})",
        series.len()
    );

    assert!(
        active_sample.cursor_effect > baseline.cursor_effect,
        "cursor-comet must have generated cursor_effect demand before disable"
    );
    assert_eq!(
        soak_presents_delta, 0,
        "frame.presents must stay constant after disabling effects (settled={settled:?}, after_soak={after_soak:?})"
    );
    assert_eq!(
        soak_cursor_effect_delta, 0,
        "frame.demand_reasons.cursor_effect must stay 0 delta after disabling effects (settled={settled:?}, after_soak={after_soak:?})"
    );
    assert_eq!(
        soak_redisplay_delta, 0,
        "frame.demand_reasons.redisplay must stay 0 delta after disabling effects (settled={settled:?}, after_soak={after_soak:?})"
    );
}
